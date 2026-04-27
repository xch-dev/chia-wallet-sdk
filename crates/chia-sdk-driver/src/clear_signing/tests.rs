use anyhow::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_test::Simulator;
use chia_sdk_types::{
    Condition, Conditions,
    puzzles::{EverythingWithSingletonTailArgs, EverythingWithSingletonTailSolution},
};
use clvm_utils::{ToTreeHash, tree_hash};
use rstest::rstest;

use crate::{
    Action, BURN_PUZZLE_HASH, CustodyInfo, Deltas, DropCoin, FeeAction, Id, Nft, ParsedAsset,
    Spend, SpendContext, Spends, TestVault, VaultOutput, parse_vault_transaction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetKind {
    Xch,
    Cat,
    RevocableCat,
}

struct IssuedAsset {
    id: Id,
    asset_id: Option<Bytes32>,
    hidden_puzzle_hash: Option<Bytes32>,
}

fn issue_asset(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    alice: &TestVault,
    asset_kind: AssetKind,
    amount: u64,
) -> Result<IssuedAsset> {
    let hidden_puzzle_hash = if matches!(asset_kind, AssetKind::RevocableCat) {
        Some(Bytes32::default())
    } else {
        None
    };

    let (id, asset_id) = if let AssetKind::Cat | AssetKind::RevocableCat = asset_kind {
        let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
            alice.info.launcher_id,
            0,
        ))?;
        let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
            alice.info.custody_hash.into(),
        ))?;

        let result = alice.spend(
            sim,
            ctx,
            &[Action::issue_cat(
                Spend::new(tail_puzzle, tail_solution),
                hidden_puzzle_hash,
                amount,
            )],
        )?;

        let asset_id = result.outputs.cats[0][0].info.asset_id;
        let id = Id::Existing(asset_id);
        (id, Some(asset_id))
    } else {
        (Id::Xch, None)
    };

    Ok(IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    })
}

fn check_asset(
    asset: &ParsedAsset,
    asset_id: Option<Bytes32>,
    hidden_puzzle_hash: Option<Bytes32>,
    amount: u64,
) {
    if let Some(asset_id) = asset_id {
        let ParsedAsset::Cat(cat) = asset else {
            panic!("Expected CAT child");
        };
        assert_eq!(cat.info.asset_id, asset_id);
        assert_eq!(cat.info.hidden_puzzle_hash, hidden_puzzle_hash);
    } else {
        assert!(matches!(asset, ParsedAsset::Xch(_)));
    }

    assert_eq!(asset.coin().amount, amount);
}

fn unwrap_nft(asset: &ParsedAsset) -> &Nft {
    let ParsedAsset::Nft(nft) = asset else {
        panic!("Expected NFT asset");
    };
    nft
}

#[rstest]
fn test_clear_signing_vault_child() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let result = alice.spend(&mut sim, &mut ctx, &[])?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(
        tx.vault_child,
        Some(VaultOutput::new(alice.info.custody_hash.into(), 1))
    );

    // We don't know the launcher id or p2 puzzle hashes, since there were no other spends to reveal them.
    assert_eq!(tx.launcher_id, None);
    assert_eq!(tx.p2_puzzle_hashes.len(), 0);
    assert_eq!(tx.spends.len(), 0);

    Ok(())
}

#[rstest]
fn test_clear_signing_vault_info() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None)],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    // Because we spent another coin, we know the launcher id and p2 puzzle hashes.
    assert_eq!(tx.launcher_id, Some(alice.info.launcher_id));
    assert_eq!(tx.p2_puzzle_hashes, vec![alice.p2_puzzle_hash]);
    assert_eq!(tx.spends.len(), 1);

    Ok(())
}

#[rstest]
fn test_clear_signing_drop_coins() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let spends = Spends::new(alice.p2_puzzle_hash);
    let vault_conditions = Conditions::new().create_coin(Bytes32::default(), 0, Memos::None);
    let result = alice.custom_spend(&mut sim, &mut ctx, &[], spends, vault_conditions)?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 0);
    assert_eq!(tx.drop_coins, [DropCoin::new(Bytes32::default(), 0)]);

    Ok(())
}

#[rstest]
#[case(0, 0)]
#[case(10, 0)]
#[case(10, 10)]
#[case(20, 0)]
#[case(20, 10)]
#[case(20, 20)]
fn test_clear_signing_reserved_fee(#[case] fee_paid: u64, #[case] reserved_fee: u64) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 50)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::Fee(FeeAction {
                amount: reserved_fee,
                reserved: true,
            }),
            Action::Fee(FeeAction {
                amount: fee_paid - reserved_fee,
                reserved: false,
            }),
        ],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.fee_paid, fee_paid);
    assert_eq!(tx.reserved_fee, reserved_fee);

    Ok(())
}

#[rstest]
fn test_clear_signing_self_transfer(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 42)?;

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 42)?;

    let memos = ctx.memos(&["Hello, world!"])?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, alice.p2_puzzle_hash, 42, memos)],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];

    check_asset(&spend.asset, asset_id, hidden_puzzle_hash, 42);
    assert!(spend.clawback.is_none());
    let CustodyInfo::P2Singleton(p2_singleton) = &spend.custody else {
        panic!("Expected P2 singleton custody");
    };
    assert_eq!(p2_singleton.launcher_id, alice.info.launcher_id);
    assert_eq!(p2_singleton.nonce, 0);
    assert_eq!(p2_singleton.p2_puzzle_hash, alice.p2_puzzle_hash);
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];

    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 42);
    assert_eq!(child.asset.coin().amount, 42);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);
    assert_eq!(child.memos.human_readable_memos, vec!["Hello, world!"]);

    Ok(())
}

#[rstest]
fn test_clear_signing_intermediate_p2_singleton() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None),
            Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None),
        ],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 2);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child_1 = &spend.children[0];
    assert_eq!(child_1.asset.coin().amount, 1);
    assert_eq!(child_1.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    let intermediate_child = &spend.children[1];
    assert_eq!(intermediate_child.asset.coin().amount, 0);
    assert_eq!(
        intermediate_child.memos.p2_puzzle_hash,
        alice.p2_puzzle_hash
    );

    let intermediate_spend = &tx.spends[1];
    assert_eq!(intermediate_spend.children.len(), 1);
    assert_eq!(
        intermediate_spend.asset.coin().coin_id(),
        intermediate_child.asset.coin().coin_id()
    );

    let child_2 = &intermediate_spend.children[0];
    assert_eq!(child_2.asset.coin().amount, 1);
    assert_eq!(child_2.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_intermediate_delegated_conditions(
    #[values(false, true)] asserted: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(alice.p2_puzzle_hash, 1, Memos::None))?;
    let puzzle_hash = tree_hash(&ctx, delegated_spend.puzzle).into();

    let actions = [
        Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None),
        Action::send(Id::Xch, puzzle_hash, 1, Memos::None),
    ];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let input_coin_id = spends.xch.items[0].asset.coin_id();
    let intermediate_coin = Coin::new(input_coin_id, puzzle_hash, 1);

    ctx.spend(intermediate_coin, delegated_spend)?;

    if asserted {
        spends
            .conditions
            .required
            .push(Condition::assert_concurrent_spend(
                intermediate_coin.coin_id(),
            ));
    }

    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), if asserted { 2 } else { 1 });

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child_1 = &spend.children[0];
    assert_eq!(child_1.asset.coin().amount, 1);
    assert_eq!(child_1.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    let intermediate_child = &spend.children[1];
    assert_eq!(intermediate_child.asset.coin().amount, 1);
    assert_eq!(intermediate_child.memos.p2_puzzle_hash, puzzle_hash);

    if asserted {
        let intermediate_spend = &tx.spends[1];
        assert_eq!(intermediate_spend.children.len(), 1);
        assert_eq!(
            intermediate_spend.asset.coin().coin_id(),
            intermediate_child.asset.coin().coin_id()
        );

        let child_2 = &intermediate_spend.children[0];
        assert_eq!(child_2.asset.coin().amount, 1);
        assert_eq!(child_2.memos.p2_puzzle_hash, alice.p2_puzzle_hash);
    }

    Ok(())
}

#[rstest]
fn test_clear_signing_chained_delegated_conditions(
    #[values(false, true)] root_asserted: bool,
    #[values(false, true)] chained_asserted: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let chained_delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(alice.p2_puzzle_hash, 1, Memos::None))?;
    let chained_puzzle_hash = tree_hash(&ctx, chained_delegated_spend.puzzle).into();

    let root_delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(chained_puzzle_hash, 1, Memos::None))?;
    let root_puzzle_hash = tree_hash(&ctx, root_delegated_spend.puzzle).into();

    let actions = [
        Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None),
        Action::send(Id::Xch, root_puzzle_hash, 1, Memos::None),
    ];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let input_coin_id = spends.xch.items[0].asset.coin_id();
    let root_coin = Coin::new(input_coin_id, root_puzzle_hash, 1);
    let chained_coin = Coin::new(root_coin.coin_id(), chained_puzzle_hash, 1);

    ctx.spend(root_coin, root_delegated_spend)?;
    ctx.spend(chained_coin, chained_delegated_spend)?;

    if root_asserted {
        spends
            .conditions
            .required
            .push(Condition::assert_concurrent_spend(root_coin.coin_id()));
    }

    if chained_asserted {
        spends
            .conditions
            .required
            .push(Condition::assert_concurrent_spend(chained_coin.coin_id()));
    }

    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(
        tx.spends.len(),
        if root_asserted && chained_asserted {
            3
        } else if root_asserted {
            2
        } else {
            1
        }
    );

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child_1 = &spend.children[0];
    assert_eq!(child_1.asset.coin().amount, 1);
    assert_eq!(child_1.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    let root_child = &spend.children[1];
    assert_eq!(root_child.asset.coin().amount, 1);
    assert_eq!(root_child.memos.p2_puzzle_hash, root_puzzle_hash);

    if root_asserted {
        let root_spend = &tx.spends[1];
        assert_eq!(root_spend.children.len(), 1);
        assert_eq!(
            root_spend.asset.coin().coin_id(),
            root_child.asset.coin().coin_id()
        );

        let chained_child = &root_spend.children[0];
        assert_eq!(chained_child.asset.coin().amount, 1);
        assert_eq!(chained_child.memos.p2_puzzle_hash, chained_puzzle_hash);

        if chained_asserted {
            let chained_spend = &tx.spends[2];
            assert_eq!(chained_spend.children.len(), 1);
            assert_eq!(
                chained_spend.asset.coin().coin_id(),
                chained_child.asset.coin().coin_id()
            );

            let child_2 = &chained_spend.children[0];
            assert_eq!(child_2.asset.coin().amount, 1);
            assert_eq!(child_2.memos.p2_puzzle_hash, alice.p2_puzzle_hash);
        }
    }

    Ok(())
}

#[rstest]
fn test_clear_signing_transfer(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
    #[values(0, 100)] fee: u64,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000 + fee)?;
    let bob_puzzle_hash = "bob".tree_hash().into();

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1000)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::send(id, bob_puzzle_hash, 1000, Memos::None),
            Action::fee(fee),
        ],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.fee_paid, fee);
    assert_eq!(tx.reserved_fee, fee);
    assert_eq!(
        tx.spends.len(),
        if matches!(asset_kind, AssetKind::Xch) || fee == 0 {
            1
        } else {
            2
        }
    );

    let spend = &tx.spends.last().unwrap();
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];

    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, bob_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_mint_nft() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;

    let result = alice.spend(&mut sim, &mut ctx, &[Action::mint_empty_nft()])?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 2);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SINGLETON_LAUNCHER_HASH.into());

    let nft_spend = &tx.spends[1];
    let parent_nft = unwrap_nft(&nft_spend.asset);
    assert_eq!(nft_spend.children.len(), 1);

    let child_nft = unwrap_nft(&nft_spend.children[0].asset);
    assert_eq!(child_nft.info.launcher_id, parent_nft.info.launcher_id);
    assert_eq!(child_nft.info.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_transfer_nft() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;

    let result = alice.spend(&mut sim, &mut ctx, &[Action::mint_empty_nft()])?;
    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;
    let nft = unwrap_nft(&tx.spends[1].children[0].asset);

    let hint = ctx.hint(BURN_PUZZLE_HASH)?;
    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Existing(nft.info.launcher_id),
            BURN_PUZZLE_HASH,
            1,
            hint,
        )],
    )?;
    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child_nft = unwrap_nft(&spend.children[0].asset);
    assert_eq!(child_nft.info.p2_puzzle_hash, BURN_PUZZLE_HASH);

    Ok(())
}
