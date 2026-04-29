use anyhow::Result;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
    singleton::SingletonStruct,
};
use chia_puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
use chia_sdk_test::Simulator;
use chia_sdk_types::{
    Condition, Conditions, announcement_id,
    conditions::{AssertPuzzleAnnouncement, CreateCoin, ReserveFee},
    puzzles::{EverythingWithSingletonTailArgs, EverythingWithSingletonTailSolution},
    tree_hash_notarized_payment,
};
use clvm_traits::{ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, tree_hash};
use rstest::rstest;

use crate::{
    Action, BURN_PUZZLE_HASH, Cat, CatInfo, CatSpend, CustodyInfo, Deltas, DriverError, DropCoin,
    FeeAction, Id, IssuanceKind, Layer, Nft, OfferPreSplitInfo, P2ConditionsOrSingleton,
    P2ConditionsOrSingletonRevealInput, P2PuzzleType, ParsedAsset, RequestedAsset, SettlementLayer,
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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

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
    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;
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
    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child_nft = unwrap_nft(&spend.children[0].asset);
    assert_eq!(child_nft.info.p2_puzzle_hash, BURN_PUZZLE_HASH);

    Ok(())
}

#[rstest]
fn test_clear_signing_p2_singleton_cat_issuance(
    #[values(None, Some(Bytes32::default()))] hidden_puzzle_hash: Option<Bytes32>,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        0,
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;
    let expected_asset_id: Bytes32 = ctx.tree_hash(tail_puzzle).into();

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::issue_cat(
            Spend::new(tail_puzzle, tail_solution),
            hidden_puzzle_hash,
            1000,
        )],
    )?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

    assert_eq!(tx.issuances.len(), 1);

    let issuance = &tx.issuances[0];
    assert_eq!(issuance.asset_id, expected_asset_id);

    let IssuanceKind::EverythingWithSingleton {
        singleton_struct_hash,
        nonce,
    } = issuance.kind
    else {
        panic!("wrong issuance kind");
    };

    assert_eq!(
        singleton_struct_hash,
        SingletonStruct::new(alice.info.launcher_id)
            .tree_hash()
            .into()
    );
    assert_eq!(nonce, 0);

    let cat_spend = tx
        .spends
        .iter()
        .find(|spend| matches!(spend.asset, ParsedAsset::Cat(_)))
        .expect("expected the eve cat spend to be verified");
    assert_eq!(cat_spend.asset.coin().coin_id(), issuance.coin_id);

    Ok(())
}

#[rstest]
fn test_clear_signing_delegated_conditions_cat_issuance() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        0,
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;
    let asset_id: Bytes32 = ctx.tree_hash(tail_puzzle).into();

    let hint = ctx.hint(alice.p2_puzzle_hash)?;
    let eve_conditions = Conditions::new()
        .run_cat_tail(tail_puzzle, tail_solution)
        .create_coin(alice.p2_puzzle_hash, 1000, hint);
    let delegated_spend = ctx.delegated_spend(eve_conditions)?;
    let delegated_puzzle_hash = tree_hash(&ctx, delegated_spend.puzzle).into();
    let eve_info = CatInfo::new(asset_id, None, delegated_puzzle_hash);

    let actions = [Action::send(
        Id::Xch,
        eve_info.puzzle_hash().into(),
        1000,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let input_coin_id = spends.xch.items[0].asset.coin_id();
    let eve_coin = Coin::new(input_coin_id, eve_info.puzzle_hash().into(), 1000);
    let eve = Cat::new(eve_coin, None, eve_info);

    Cat::spend_all(&mut ctx, &[CatSpend::new(eve, delegated_spend)])?;

    spends
        .conditions
        .required
        .push(Condition::assert_concurrent_spend(eve_coin.coin_id()));

    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

    assert_eq!(tx.issuances.len(), 1);

    let issuance = &tx.issuances[0];
    assert_eq!(issuance.asset_id, asset_id);

    let IssuanceKind::EverythingWithSingleton {
        singleton_struct_hash,
        nonce,
    } = issuance.kind
    else {
        panic!("wrong issuance kind");
    };

    assert_eq!(
        singleton_struct_hash,
        SingletonStruct::new(alice.info.launcher_id)
            .tree_hash()
            .into()
    );
    assert_eq!(nonce, 0);

    let cat_spend = tx
        .spends
        .iter()
        .find(|spend| matches!(spend.asset, ParsedAsset::Cat(_)))
        .expect("expected the eve cat spend to be verified");
    assert_eq!(cat_spend.asset.coin().coin_id(), issuance.coin_id);

    Ok(())
}

// =============================================================================================
// Offer pre-split tests
// =============================================================================================

/// Compute the curried `P2ConditionsOrSingleton` puzzle hash for the given vault and fixed
/// conditions, and the cached delegated puzzle hash that the parser stores on the reveal.
fn p2_conditions_or_singleton_hash(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    nonce: usize,
    fixed_conditions: &[Condition],
) -> Result<(Bytes32, Bytes32)> {
    let quoted = clvm_quote!(fixed_conditions).to_clvm(ctx)?;
    let fixed_delegated_puzzle_hash: Bytes32 = tree_hash(ctx, quoted).into();
    let p2cs = P2ConditionsOrSingleton::new(launcher_id, nonce, fixed_delegated_puzzle_hash);
    Ok((p2cs.tree_hash().into(), fixed_delegated_puzzle_hash))
}

/// Build a settlement-puzzle reveal (parent_coin_info = 0) that the parser can use to resolve
/// asserted requested payments. We return a free-standing [`CoinSpend`] instead of pushing into
/// the [`SpendContext`] so the simulator never sees this synthetic reveal — its job is purely to
/// describe the offer's requested side to `parse_vault_transaction`.
fn settlement_payment_reveal(
    ctx: &mut SpendContext,
    notarized_payment: NotarizedPayment,
) -> Result<CoinSpend> {
    let puzzle = SettlementLayer.construct_puzzle(ctx)?;
    let solution = SettlementLayer.construct_solution(
        ctx,
        SettlementPaymentsSolution::new(vec![notarized_payment]),
    )?;
    let puzzle_reveal = ctx.serialize(&puzzle)?;
    let solution = ctx.serialize(&solution)?;
    let coin = Coin::new(Bytes32::default(), SETTLEMENT_PAYMENT_HASH.into(), 0);
    Ok(CoinSpend::new(coin, puzzle_reveal, solution))
}

#[rstest]
#[case(0)]
#[case(123)]
fn test_clear_signing_offer_pre_split_xch(#[case] fee: u64) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let amount: u64 = 1000;
    let settlement_amount: u64 = amount - fee;

    let alice = TestVault::mint(&mut sim, &mut ctx, amount)?;

    // The offer requests 100 XCH paid back to alice's p2 puzzle hash.
    let np = NotarizedPayment::new(
        Bytes32::new([7; 32]),
        vec![Payment::new(alice.p2_puzzle_hash, 100, Memos::None)],
    );
    let np_hash = tree_hash_notarized_payment(&ctx, &np);
    let asserted_announcement = announcement_id(SETTLEMENT_PAYMENT_HASH.into(), np_hash);

    // Pre-split fixed conditions: pay `settlement_amount` to settlement, reserve `fee` if any,
    // and assert the requested payment will be paid back.
    let mut fixed_conditions = vec![
        Condition::CreateCoin(CreateCoin::new(
            SETTLEMENT_PAYMENT_HASH.into(),
            settlement_amount,
            Memos::None,
        )),
        Condition::AssertPuzzleAnnouncement(AssertPuzzleAnnouncement {
            announcement_id: asserted_announcement,
        }),
    ];
    if fee > 0 {
        fixed_conditions.push(Condition::ReserveFee(ReserveFee::new(fee)));
    }

    let nonce = 0;
    let (p2cs_hash, _) = p2_conditions_or_singleton_hash(
        &mut ctx,
        alice.info.launcher_id,
        nonce,
        &fixed_conditions,
    )?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2cs_hash, amount, Memos::None)],
    )?;

    // Build a settlement reveal *after* the simulator has accepted the real transaction so it
    // doesn't try to validate this synthetic parent-zero spend.
    let mut coin_spends = result.coin_spends;
    coin_spends.push(settlement_payment_reveal(&mut ctx, np.clone())?);

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        coin_spends,
        vec![],
        vec![P2ConditionsOrSingletonRevealInput {
            launcher_id: alice.info.launcher_id,
            nonce,
            fixed_conditions: fixed_conditions.clone(),
        }],
    )?;

    let linked_offer = tx
        .linked_offer
        .as_ref()
        .expect("expected a linked offer to be reported");
    assert_eq!(linked_offer.reserved_fee, fee);
    assert_eq!(linked_offer.requested_payments.len(), 1);
    assert!(matches!(
        linked_offer.requested_payments[0].asset,
        RequestedAsset::Xch
    ));
    assert_eq!(
        linked_offer.requested_payments[0].notarized_payment.nonce,
        np.nonce
    );

    // The pre-split should not be folded into the main transaction's `received_payments` /
    // `reserved_fee` — those are for the current transaction, not the future offer.
    assert!(tx.received_payments.is_empty());
    assert_eq!(tx.reserved_fee, 0);

    // The pre-split child should carry the OfferPreSplit classification.
    let pre_split = tx
        .spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .find(|child| matches!(child.p2_puzzle_type, P2PuzzleType::OfferPreSplit(_)))
        .expect("expected the pre-split child to be classified as OfferPreSplit");

    let P2PuzzleType::OfferPreSplit(OfferPreSplitInfo {
        launcher_id,
        nonce: info_nonce,
        settlement_amount: info_settlement_amount,
        ..
    }) = &pre_split.p2_puzzle_type
    else {
        unreachable!();
    };
    assert_eq!(*launcher_id, alice.info.launcher_id);
    assert_eq!(*info_nonce, nonce);
    assert_eq!(*info_settlement_amount, settlement_amount);
    assert_eq!(pre_split.asset.coin().amount, amount);

    Ok(())
}

#[rstest]
fn test_clear_signing_recognizes_settlement_and_burn() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::send(Id::Xch, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
            Action::send(Id::Xch, BURN_PUZZLE_HASH, 1, Memos::None),
        ],
    )?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![],
    )?;

    let kinds: Vec<_> = tx
        .spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .map(|child| match &child.p2_puzzle_type {
            P2PuzzleType::Offered => "offered",
            P2PuzzleType::Burned => "burned",
            P2PuzzleType::OfferPreSplit(_) => "pre_split",
            P2PuzzleType::Unknown => "unknown",
        })
        .collect();

    assert!(kinds.contains(&"offered"));
    assert!(kinds.contains(&"burned"));
    assert!(tx.linked_offer.is_none());

    Ok(())
}

#[rstest]
fn test_clear_signing_offer_pre_split_launcher_mismatch() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    // Use a different launcher id than alice's vault.
    let other_launcher = Bytes32::new([99; 32]);

    let fixed_conditions = vec![Condition::CreateCoin(CreateCoin::new(
        SETTLEMENT_PAYMENT_HASH.into(),
        1000,
        Memos::None,
    ))];

    let nonce = 0;
    let (p2cs_hash, _) =
        p2_conditions_or_singleton_hash(&mut ctx, other_launcher, nonce, &fixed_conditions)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2cs_hash, 1000, Memos::None)],
    )?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![P2ConditionsOrSingletonRevealInput {
            launcher_id: other_launcher,
            nonce,
            fixed_conditions,
        }],
    );

    assert!(matches!(tx, Err(DriverError::LinkedOfferLauncherMismatch)));

    Ok(())
}

#[rstest]
fn test_clear_signing_offer_pre_split_unbalanced_fee() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    // Settlement amount of 900 with no `ReserveFee` for the missing 100 — the pre-split spend
    // wouldn't balance, so we should refuse to surface this as a real linked offer.
    let fixed_conditions = vec![Condition::CreateCoin(CreateCoin::new(
        SETTLEMENT_PAYMENT_HASH.into(),
        900,
        Memos::None,
    ))];

    let nonce = 0;
    let (p2cs_hash, _) = p2_conditions_or_singleton_hash(
        &mut ctx,
        alice.info.launcher_id,
        nonce,
        &fixed_conditions,
    )?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2cs_hash, 1000, Memos::None)],
    )?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![P2ConditionsOrSingletonRevealInput {
            launcher_id: alice.info.launcher_id,
            nonce,
            fixed_conditions,
        }],
    );

    assert!(matches!(tx, Err(DriverError::LinkedOfferFeeMismatch)));

    Ok(())
}

#[rstest]
fn test_clear_signing_offer_pre_split_announcement_mismatch() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2000)?;

    // Two pre-split coins, each valid on its own but with disagreeing announcement sets, which
    // means they really describe two different offers — a state we refuse to surface as one.
    let mk_fixed = |announcement: Bytes32| -> Vec<Condition> {
        vec![
            Condition::CreateCoin(CreateCoin::new(
                SETTLEMENT_PAYMENT_HASH.into(),
                1000,
                Memos::None,
            )),
            Condition::AssertPuzzleAnnouncement(AssertPuzzleAnnouncement {
                announcement_id: announcement,
            }),
        ]
    };

    let fixed_a = mk_fixed(Bytes32::new([1; 32]));
    let fixed_b = mk_fixed(Bytes32::new([2; 32]));

    let nonce = 0;
    let (hash_a, _) =
        p2_conditions_or_singleton_hash(&mut ctx, alice.info.launcher_id, nonce, &fixed_a)?;
    let (hash_b, _) =
        p2_conditions_or_singleton_hash(&mut ctx, alice.info.launcher_id, nonce, &fixed_b)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[
            Action::send(Id::Xch, hash_a, 1000, Memos::None),
            Action::send(Id::Xch, hash_b, 1000, Memos::None),
        ],
    )?;

    let tx = parse_vault_transaction(
        &mut ctx,
        result.delegated_spend,
        result.coin_spends,
        vec![],
        vec![
            P2ConditionsOrSingletonRevealInput {
                launcher_id: alice.info.launcher_id,
                nonce,
                fixed_conditions: fixed_a,
            },
            P2ConditionsOrSingletonRevealInput {
                launcher_id: alice.info.launcher_id,
                nonce,
                fixed_conditions: fixed_b,
            },
        ],
    );

    assert!(matches!(
        tx,
        Err(DriverError::LinkedOfferAnnouncementMismatch)
    ));

    Ok(())
}
