use anyhow::Result;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
    singleton::SingletonStruct,
};
use chia_puzzles::{SETTLEMENT_PAYMENT, SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
use chia_sdk_test::Simulator;
use chia_sdk_types::{
    Condition, Conditions, MessageFlags, MessageSide, announcement_id,
    puzzles::{
        EverythingWithSingletonTailArgs, EverythingWithSingletonTailSolution, SettlementPayment,
    },
    tree_hash_notarized_payment,
};
use clvm_traits::clvm_list;
use clvm_utils::{ToTreeHash, tree_hash};
use rstest::rstest;

use crate::{
    Action, AssertedRequestedPayment, BURN_PUZZLE_HASH, Bulletin, BulletinMessage, Cat, CatInfo,
    CatSpend, ClawbackInfo, ClawbackPath, ClawbackV2, CustodyInfo, Deltas, DriverError, DropCoin,
    FeeAction, Id, IssuanceKind, LinkedOffer, Nft, OfferPreSplitInfo, P2ConditionsOrSingleton,
    ParsedAsset, Puzzle, RequestedAsset, Reveals, Spend, SpendContext, SpendKind, Spends,
    TestP2Puzzle, TestVault, TransferType, VaultOutput, iter_final_children,
    parse_vault_transaction,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(
        tx.vault_child,
        Some(VaultOutput::new(alice.info.custody_hash.into(), 1))
    );
    assert_eq!(tx.p2_puzzle_hashes, vec![alice.p2_puzzle_hash]);
    assert_eq!(tx.spends.len(), 0);

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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let final_children = iter_final_children(&tx.spends);
    assert_eq!(final_children.count(), 2);

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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
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

#[rstest]
fn test_clear_signing_bulletin() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    let mut deltas = Deltas::new();
    deltas.set_needed(Id::Xch);
    alice.select_coins(&sim, &mut spends, &deltas)?;

    let parent_coin_id = spends.xch.items[0].asset.coin_id();
    let messages = vec![
        BulletinMessage::new("First".to_string(), "This is the first message".to_string()),
        BulletinMessage::new(
            "Second".to_string(),
            "This is the second message".to_string(),
        ),
    ];

    let (parent_conditions, bulletin) =
        Bulletin::create(parent_coin_id, alice.p2_puzzle_hash, messages)?;

    let SpendKind::Conditions(kind) = &mut spends.xch.items[0].kind else {
        panic!("expected conditions spend");
    };

    kind.add_conditions(parent_conditions);

    let TestP2Puzzle::P2Singleton(p2_singleton) = alice.p2_puzzles[&alice.p2_puzzle_hash.into()]
    else {
        panic!("expected p2 singleton");
    };

    let bulletin_conditions = bulletin.conditions(&mut ctx)?;
    let delegated_spend = ctx.delegated_spend(bulletin_conditions)?;
    let delegated_puzzle_hash = tree_hash(&ctx, delegated_spend.puzzle);
    let p2_spend =
        p2_singleton.spend(&mut ctx, alice.info.custody_hash.into(), 1, delegated_spend)?;
    bulletin.spend(&mut ctx, p2_spend)?;

    let vault_conditions = Conditions::new().send_message(
        MessageFlags::PUZZLE.encode(MessageSide::Sender)
            | MessageFlags::COIN.encode(MessageSide::Receiver),
        delegated_puzzle_hash.to_vec().into(),
        vec![ctx.alloc(&bulletin.coin.coin_id())?],
    );
    let result = alice.custom_spend(&mut sim, &mut ctx, &[], spends, vault_conditions)?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 2);

    // The second spend is actually the parent that created the bulletin, due to the ordering of spends above.
    let spend = &tx.spends[1];

    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];

    // The child is not parsed as a bulletin here, but since it's ephemeral and 0 amount it probably doesn't matter much.
    // We can revisit this later if it's necessary to parse this.
    check_asset(&child.asset, None, None, 0);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, bulletin.coin.puzzle_hash);

    // The first spend is the bulletin itself, and the more interesting thing to check.
    let ParsedAsset::Bulletin(parsed) = &tx.spends[0].asset else {
        panic!("Expected bulletin asset");
    };
    assert_eq!(parsed, &bulletin);

    Ok(())
}

#[rstest]
fn test_clear_signing_create_clawback(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1)?;

    let clawback = ClawbackV2::new(
        alice.p2_puzzle_hash,
        bob.p2_puzzle_hash,
        1000,
        1,
        asset_id.is_some(),
    );
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, clawback.tree_hash().into(), 1, memos)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1);
    assert_eq!(child.memos.clawback, Some(clawback));
    assert_eq!(child.memos.p2_puzzle_hash, bob.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_claw_back(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1)?;

    let clawback = ClawbackV2::new(
        alice.p2_puzzle_hash,
        bob.p2_puzzle_hash,
        1000,
        1,
        asset_id.is_some(),
    );
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, clawback.tree_hash().into(), 1, memos)],
    )?;

    alice
        .p2_puzzles
        .insert(clawback.tree_hash(), TestP2Puzzle::Clawback(clawback));

    let actions = [Action::send(id, alice.p2_puzzle_hash, 1, Memos::None)];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let vault_conditions = Conditions::new().assert_before_seconds_absolute(clawback.seconds);
    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    check_asset(&spend.asset, asset_id, hidden_puzzle_hash, 1);
    assert_eq!(
        spend.clawback,
        Some(ClawbackInfo::new(clawback, ClawbackPath::Sender))
    );
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_spend_clawback(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let mut bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1)?;

    let clawback = ClawbackV2::new(
        alice.p2_puzzle_hash,
        bob.p2_puzzle_hash,
        1000,
        1,
        asset_id.is_some(),
    );
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, clawback.tree_hash().into(), 1, memos)],
    )?;

    sim.set_next_timestamp(2000)?;

    bob.p2_puzzles
        .insert(clawback.tree_hash(), TestP2Puzzle::Clawback(clawback));

    let result = bob.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, alice.p2_puzzle_hash, 1, Memos::None)],
    )?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    check_asset(&spend.asset, asset_id, hidden_puzzle_hash, 1);
    assert_eq!(
        spend.clawback,
        Some(ClawbackInfo::new(clawback, ClawbackPath::Receiver))
    );
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_unasserted_claw_back_timestamp() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, false);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    alice
        .p2_puzzles
        .insert(clawback.tree_hash(), TestP2Puzzle::Clawback(clawback));

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None)],
    )?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let result = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(
        result.unwrap_err(),
        DriverError::UnguaranteedClawBack
    ));

    Ok(())
}

#[rstest]
fn test_clear_signing_unrevealed_clawback() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, false);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    alice
        .p2_puzzles
        .insert(clawback.tree_hash(), TestP2Puzzle::Clawback(clawback));

    let actions = [Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None)];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let vault_conditions = Conditions::new().assert_before_seconds_absolute(clawback.seconds);
    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let result = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(
        result.unwrap_err(),
        DriverError::InvalidLinkedCustody
    ));

    Ok(())
}

#[rstest]
#[case(Bytes32::default(), TransferType::Sent)]
#[case(BURN_PUZZLE_HASH, TransferType::Burned)]
#[case(SETTLEMENT_PAYMENT_HASH.into(), TransferType::Offered)]
fn test_clear_signing_transfer_type(
    #[case] p2_puzzle_hash: Bytes32,
    #[case] transfer_type: TransferType,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2_puzzle_hash, 1, Memos::None)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.memos.p2_puzzle_hash, p2_puzzle_hash);
    assert_eq!(child.transfer_type, transfer_type);

    Ok(())
}

#[rstest]
fn test_clear_signing_create_p2_conditions_or_singleton(
    #[values(false, true)] revealed_up_front: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let conditions = Conditions::new()
        .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 750, Memos::None)
        .reserve_fee(250);
    let delegated_spend = ctx.delegated_spend(conditions.clone())?;
    let conditions_hash = ctx.tree_hash(delegated_spend.puzzle).into();
    let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
        alice.info.launcher_id,
        0,
        conditions_hash,
    );
    let memos = ctx.memos(&clvm_list!(alice.p2_puzzle_hash, &conditions))?;

    alice
        .p2_puzzles
        .insert(p2.tree_hash(), TestP2Puzzle::P2ConditionsOrSingleton(p2));

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2.tree_hash().into(), 1000, memos)],
    )?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;

    if revealed_up_front {
        reveals.reveal_p2_conditions_or_singleton(p2);
    }

    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.memos.p2_puzzle_hash, p2.tree_hash().into());

    if revealed_up_front {
        assert_eq!(
            child.transfer_type,
            TransferType::OfferPreSplit(OfferPreSplitInfo {
                launcher_id: p2.launcher_id,
                nonce: p2.nonce,
                fixed_conditions: conditions.into_vec(),
                settlement_amount: 750,
            })
        );

        assert_eq!(
            tx.linked_offer,
            Some(LinkedOffer {
                requested_payments: vec![],
                reserved_fee: 250,
            })
        );
    } else {
        assert_eq!(child.transfer_type, TransferType::Sent);

        assert_eq!(tx.linked_offer, None);
    }

    Ok(())
}

#[rstest]
fn test_clear_signing_spend_p2_conditions_or_singleton_fixed() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let conditions = Conditions::new()
        .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 750, Memos::None)
        .reserve_fee(250);
    let delegated_spend = ctx.delegated_spend(conditions.clone())?;
    let conditions_hash = ctx.tree_hash(delegated_spend.puzzle).into();
    let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
        alice.info.launcher_id,
        0,
        conditions_hash,
    );

    alice
        .p2_puzzles
        .insert(p2.tree_hash(), TestP2Puzzle::P2ConditionsOrSingleton(p2));

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            p2.tree_hash().into(),
            1000,
            Memos::None,
        )],
    )?;

    let coin = result.outputs.xch[0];

    let mut spends = Spends::new(bob.p2_puzzle_hash);

    let fixed_spend = p2.fixed_spend(&mut ctx, delegated_spend)?;
    ctx.spend(coin, fixed_spend)?;

    spends.add(Coin::new(
        coin.coin_id(),
        SETTLEMENT_PAYMENT_HASH.into(),
        750,
    ));

    let result = bob.custom_spend(&mut sim, &mut ctx, &[], spends, Conditions::new())?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    // This is because the intermediate coin used to assert the puzzle announcement has an amount of 1.
    // It was created by the settlement coin itself, so it's unknown where this coin came from.
    // Even though it's ephemeral and not technically a fee being paid, it gets counted here conservatively.
    assert_eq!(tx.fee_paid, 1);

    // We didn't actually pay any fees, so this should be 0.
    assert_eq!(tx.reserved_fee, 0);

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 0);

    assert_eq!(tx.received_payments.len(), 1);

    let payment = &tx.received_payments[0];
    assert_eq!(payment.asset, RequestedAsset::Xch);
    assert_eq!(payment.notarized_payment.payments.len(), 1);

    let payment = &payment.notarized_payment.payments[0];
    assert_eq!(payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(payment.amount, 750);

    Ok(())
}

#[rstest]
fn test_clear_signing_spend_p2_conditions_or_singleton_vault() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let mut alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let conditions = Conditions::new()
        .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 750, Memos::None)
        .reserve_fee(250);
    let delegated_spend = ctx.delegated_spend(conditions.clone())?;
    let conditions_hash = ctx.tree_hash(delegated_spend.puzzle).into();
    let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
        alice.info.launcher_id,
        0,
        conditions_hash,
    );
    let memos = ctx.memos(&clvm_list!(alice.p2_puzzle_hash, &conditions))?;

    alice
        .p2_puzzles
        .insert(p2.tree_hash(), TestP2Puzzle::P2ConditionsOrSingleton(p2));

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, p2.tree_hash().into(), 1000, memos)],
    )?;

    let actions = [Action::send(
        Id::Xch,
        alice.p2_puzzle_hash,
        1000,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    spends
        .conditions
        .required
        .push(Condition::assert_my_coin_id(
            spends.xch.items[0].asset.coin_id(),
        ));
    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_p2_conditions_or_singleton(p2);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.asset.coin().amount, 1000);
    assert_eq!(spend.asset.coin().puzzle_hash, p2.tree_hash().into());
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_offer() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let IssuedAsset { id, asset_id, .. } =
        issue_asset(&mut sim, &mut ctx, &alice, AssetKind::Cat, 1000)?;

    let notarized_payment = NotarizedPayment::new(
        Bytes32::default(),
        vec![Payment::new(alice.p2_puzzle_hash, 1000, Memos::None)],
    );

    let notarized_payment_hash = tree_hash_notarized_payment(&ctx, &notarized_payment);

    let settlement_solution = ctx.alloc(&SettlementPaymentsSolution::new(vec![
        notarized_payment.clone(),
    ]))?;

    let requested_payment = CoinSpend::new(
        Coin::new(Bytes32::default(), SETTLEMENT_PAYMENT_HASH.into(), 1000),
        SETTLEMENT_PAYMENT.to_vec().into(),
        ctx.serialize(&settlement_solution)?,
    );

    // Make the offer
    let actions = [Action::send(
        id,
        SETTLEMENT_PAYMENT_HASH.into(),
        1000,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    spends
        .conditions
        .required
        .push(Condition::assert_puzzle_announcement(announcement_id(
            SETTLEMENT_PAYMENT_HASH.into(),
            notarized_payment_hash,
        )));

    let result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;
    let coin_spends = [
        result.spend_bundle.coin_spends.clone(),
        vec![requested_payment],
    ]
    .concat();
    let reveals = Reveals::from_coin_spends(&mut ctx, &coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.received_payments.len(), 1);

    let payment = &tx.received_payments[0];
    assert_eq!(payment.asset, RequestedAsset::Xch);
    assert_eq!(payment.notarized_payment, notarized_payment);

    // Take the offer
    let settlement_cat = result.outputs.cats[0][0];
    let maker_bundle = result.spend_bundle;

    let actions = [Action::settle(Id::Xch, notarized_payment.clone())];

    let mut spends = Spends::new(bob.p2_puzzle_hash);
    spends.add(settlement_cat);
    bob.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let result =
        bob.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    sim.new_transaction(SpendBundle::new(
        [maker_bundle.coin_spends, result.spend_bundle.coin_spends].concat(),
        maker_bundle.aggregated_signature + &result.spend_bundle.aggregated_signature,
    ))?;

    assert_eq!(tx.fee_paid, 1000); // TODO: This isn't ideal
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.received_payments.len(), 2);

    let xch_payment = &tx.received_payments[0];
    assert_eq!(xch_payment.asset, RequestedAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    let cat_payment = &tx.received_payments[1];
    assert_eq!(
        cat_payment.asset,
        RequestedAsset::Cat {
            asset_id: asset_id.unwrap(),
            hidden_puzzle_hash: None,
        }
    );
    assert_eq!(cat_payment.notarized_payment.payments.len(), 1);

    let cat_payment = &cat_payment.notarized_payment.payments[0];
    assert_eq!(cat_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(cat_payment.amount, 1000);

    Ok(())
}

#[rstest]
fn test_clear_signing_single_sided_offer() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let IssuedAsset { id, asset_id, .. } =
        issue_asset(&mut sim, &mut ctx, &alice, AssetKind::Cat, 1000)?;

    // Make the offer
    let result = alice.partial_spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            id,
            SETTLEMENT_PAYMENT_HASH.into(),
            1000,
            Memos::None,
        )],
    )?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.received_payments.len(), 0);

    // Take the offer
    let settlement_cat = result.outputs.cats[0][0];
    let maker_bundle = result.spend_bundle;

    let actions = [];

    let mut spends = Spends::new(bob.p2_puzzle_hash);
    spends.add(settlement_cat);
    bob.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let result =
        bob.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    sim.new_transaction(SpendBundle::new(
        [maker_bundle.coin_spends, result.spend_bundle.coin_spends].concat(),
        maker_bundle.aggregated_signature + &result.spend_bundle.aggregated_signature,
    ))?;

    assert_eq!(tx.fee_paid, 1); // TODO: This isn't ideal
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 0);

    assert_eq!(tx.received_payments.len(), 1);

    let cat_payment = &tx.received_payments[0];
    assert_eq!(
        cat_payment.asset,
        RequestedAsset::Cat {
            asset_id: asset_id.unwrap(),
            hidden_puzzle_hash: None,
        }
    );
    assert_eq!(cat_payment.notarized_payment.payments.len(), 1);

    let cat_payment = &cat_payment.notarized_payment.payments[0];
    assert_eq!(cat_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(cat_payment.amount, 1000);

    Ok(())
}

#[rstest]
fn test_clear_signing_pre_split_offer() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let IssuedAsset { id, asset_id, .. } =
        issue_asset(&mut sim, &mut ctx, &alice, AssetKind::Cat, 1000)?;

    let notarized_payment = NotarizedPayment::new(
        Bytes32::default(),
        vec![Payment::new(alice.p2_puzzle_hash, 1000, Memos::None)],
    );

    let notarized_payment_hash = tree_hash_notarized_payment(&ctx, &notarized_payment);

    let settlement_puzzle = ctx.alloc_mod::<SettlementPayment>()?;
    let settlement_puzzle = Puzzle::parse(&ctx, settlement_puzzle);
    let settlement_solution = ctx.alloc(&SettlementPaymentsSolution::new(vec![
        notarized_payment.clone(),
    ]))?;

    let conditions = Conditions::new()
        .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 750, Memos::None)
        .assert_puzzle_announcement(announcement_id(
            SETTLEMENT_PAYMENT_HASH.into(),
            notarized_payment_hash,
        ));
    let delegated_spend = ctx.delegated_spend(conditions.clone())?;
    let delegated_puzzle_hash = ctx.tree_hash(delegated_spend.puzzle).into();

    let p2 = P2ConditionsOrSingleton::from_quoted_conditions_hash(
        alice.info.launcher_id,
        0,
        delegated_puzzle_hash,
    );
    let memos = ctx.memos(&clvm_list!(alice.p2_puzzle_hash, &conditions))?;

    // Make the offer
    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, p2.tree_hash().into(), 750, memos)],
    )?;
    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_p2_conditions_or_singleton(p2);
    reveals.reveal_settlement_payment(&mut ctx, settlement_puzzle, settlement_solution)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 750);
    assert_eq!(child.memos.p2_puzzle_hash, p2.tree_hash().into());
    assert_eq!(
        child.transfer_type,
        TransferType::OfferPreSplit(OfferPreSplitInfo {
            launcher_id: p2.launcher_id,
            nonce: p2.nonce,
            fixed_conditions: conditions.into_vec(),
            settlement_amount: 750,
        })
    );

    let child = &spend.children[1];
    assert_eq!(child.asset.coin().amount, 250);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    assert_eq!(tx.received_payments.len(), 0);

    assert_eq!(
        tx.linked_offer,
        Some(LinkedOffer {
            requested_payments: vec![AssertedRequestedPayment {
                asset: RequestedAsset::Xch,
                notarized_payment: notarized_payment.clone(),
            }],
            reserved_fee: 0,
        })
    );

    // Take the offer
    let pre_split_cat = result.outputs.cats[0][0];

    let fixed_spend = p2.fixed_spend(&mut ctx, delegated_spend)?;
    let settlement_cats = Cat::spend_all(&mut ctx, &[CatSpend::new(pre_split_cat, fixed_spend)])?;

    let actions = [Action::settle(Id::Xch, notarized_payment.clone())];

    let mut spends = Spends::new(bob.p2_puzzle_hash);

    for settlement_cat in settlement_cats {
        spends.add(settlement_cat);
    }

    bob.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let result = bob.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.fee_paid, 1000); // TODO: This isn't ideal
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.received_payments.len(), 2);

    let xch_payment = &tx.received_payments[0];
    assert_eq!(xch_payment.asset, RequestedAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    let cat_payment = &tx.received_payments[1];
    assert_eq!(
        cat_payment.asset,
        RequestedAsset::Cat {
            asset_id: asset_id.unwrap(),
            hidden_puzzle_hash: None,
        }
    );
    assert_eq!(cat_payment.notarized_payment.payments.len(), 1);

    let cat_payment = &cat_payment.notarized_payment.payments[0];
    assert_eq!(cat_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(cat_payment.amount, 750);

    Ok(())
}

#[rstest]
fn test_clear_signing_wrong_launcher_id() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, bob.p2_puzzle_hash, 1000, Memos::None)],
    )?;
    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.asset.coin().amount, 1000);
    assert_eq!(spend.asset.coin().puzzle_hash, alice.p2_puzzle_hash);
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, bob.p2_puzzle_hash);

    let alice_coin_spends = result.spend_bundle.coin_spends;

    let result = bob.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(
            Id::Xch,
            alice.p2_puzzle_hash,
            1000,
            Memos::None,
        )],
    )?;
    let coin_spends = [alice_coin_spends, result.spend_bundle.coin_spends].concat();
    let reveals = Reveals::from_coin_spends(&mut ctx, &coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.asset.coin().amount, 1000);
    assert_eq!(spend.asset.coin().puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, alice.p2_puzzle_hash);

    Ok(())
}
