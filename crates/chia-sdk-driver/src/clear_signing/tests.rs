use anyhow::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    CoinProof, Memos,
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
    singleton::SingletonStruct,
};
use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT, SETTLEMENT_PAYMENT_HASH,
    SINGLETON_LAUNCHER_HASH,
};
use chia_sdk_test::Simulator;
use chia_sdk_types::{
    Condition, Conditions, MessageFlags, MessageSide, Mod, announcement_id,
    conditions::TradePrice,
    puzzles::{
        EverythingWithSingletonTailArgs, EverythingWithSingletonTailSolution, P2OneOfManySolution,
        RevocationArgs, SettlementPayment,
    },
    tree_hash_notarized_payment,
};
use clvm_traits::{clvm_list, clvm_quote};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::NodePtr;
use rstest::rstest;

use crate::{
    Action, AssetFlow, BURN_PUZZLE_HASH, BagAsset, BagCoin, BagFormat, BagPayment, Bulletin,
    BulletinMessage, Cat, CatInfo, CatSpend, ClawbackInfo, ClawbackPath, ClawbackV2,
    ClearSigningAsset, CustodyInfo, Delta, Deltas, DriverError, DropCoin, FeeAction, HashedPtr, Id,
    IssuanceKind, Layer, LinkedOffer, Nft, NftInfo, OfferPreSplitInfo, OptionInfo,
    P2ConditionsOrSingleton, P2ConditionsOrSingletonVersion, ParsedAsset, ParsedBagPayment,
    ParsedChild, Puzzle, Reveals, SecureTheBag, SecureTheBagInfo, SingleCatSpend, Spend,
    SpendContext, SpendKind, Spends, TestP2Puzzle, TestVault, TransactionData, TransferNftById,
    TransferType, VaultOutput, VaultTransaction, get_extra_delta_message, iter_final_children,
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
            Bytes::default(),
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

        let reveals = Reveals::from_coin_spends(ctx, &result.spend_bundle.coin_spends)?;
        let tx =
            parse_vault_transaction(reveals, ctx, alice.info.launcher_id, result.delegated_spend)?;

        let asset_id = result.outputs.cats[0][0].info.asset_id;
        let id = Id::Existing(asset_id);

        assert_eq!(tx.issuances.len(), 1);

        let issuance = &tx.issuances[0];
        assert_eq!(issuance.asset_id, asset_id);
        assert_eq!(issuance.extra_delta, 0);
        assert!(issuance.verified);

        let xch = xch_asset_flow(&tx.asset_flows);
        assert_eq!(xch.issued_amount, 0);
        assert_eq!(xch.melted_amount, u128::from(amount));
        assert_eq!(xch.unaccounted_amount, 0);

        let cat = cat_asset_flow(&tx.asset_flows);
        assert_eq!(cat.issued_amount, u128::from(amount));
        assert_eq!(cat.melted_amount, 0);
        assert_eq!(cat.unaccounted_amount, 0);

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

fn asset_flow(flows: &[AssetFlow], asset: ClearSigningAsset) -> Option<&AssetFlow> {
    flows.iter().find(|flow| flow.asset == asset)
}

fn xch_asset_flow(flows: &[AssetFlow]) -> &AssetFlow {
    flows
        .iter()
        .find(|flow| matches!(flow.asset, ClearSigningAsset::Xch))
        .unwrap()
}

fn cat_asset_flow(flows: &[AssetFlow]) -> &AssetFlow {
    flows
        .iter()
        .find(|flow| matches!(flow.asset, ClearSigningAsset::Cat { .. }))
        .unwrap()
}

fn nft_asset_flow<'a>(flows: &'a [AssetFlow], nft: &Nft) -> &'a AssetFlow {
    asset_flow(
        flows,
        ClearSigningAsset::Nft {
            launcher_id: nft.info.launcher_id,
            metadata: nft.info.metadata,
            metadata_updater_puzzle_hash: nft.info.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: nft.info.royalty_puzzle_hash,
            royalty_basis_points: nft.info.royalty_basis_points,
        },
    )
    .unwrap()
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
fn test_clear_signing_xch_funded_drop_coin() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let actions = [Action::Fee(FeeAction {
        amount: 2,
        reserved: false,
    })];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let drop_coin = DropCoin::new(Bytes32::default(), 2);
    let vault_conditions =
        Conditions::new().create_coin(drop_coin.puzzle_hash, drop_coin.amount, Memos::None);
    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.drop_coins, [drop_coin]);

    let xch = xch_asset_flow(&tx.asset_flows);
    assert_eq!(xch.unaccounted_amount, 0);

    Ok(())
}

#[rstest]
fn test_clear_signing_mint_two_nfts_from_drop_coin() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let actions = [Action::Fee(FeeAction {
        amount: 2,
        reserved: false,
    })];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let drop_coin = DropCoin::new(alice.p2_puzzle_hash, 2);
    let vault_conditions =
        Conditions::new().create_coin(drop_coin.puzzle_hash, drop_coin.amount, Memos::None);
    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;
    assert_eq!(tx.drop_coins, [drop_coin]);

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::mint_empty_nft(), Action::mint_empty_nft()],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let xch = xch_asset_flow(&tx.asset_flows);
    assert_eq!(xch.input_amount, 2);
    assert_eq!(xch.melted_amount, 2);
    assert_eq!(xch.unaccounted_amount, 0);

    let nfts: Vec<&Nft> = tx
        .spends
        .iter()
        .filter_map(|spend| match &spend.asset {
            ParsedAsset::Nft(_) => Some(unwrap_nft(&spend.children[0].asset)),
            _ => None,
        })
        .collect();
    assert_eq!(nfts.len(), 2);

    for nft in nfts {
        assert_eq!(nft_asset_flow(&tx.asset_flows, nft).unaccounted_amount, 0);
    }

    Ok(())
}

#[rstest]
#[case(0, 0)]
#[case(10, 0)]
#[case(10, 10)]
#[case(20, 0)]
#[case(20, 10)]
#[case(20, 20)]
fn test_clear_signing_reserved_fee(
    #[case] total_fee: u64,
    #[case] reserved_fee: u64,
) -> Result<()> {
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
                amount: total_fee - reserved_fee,
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

    assert_eq!(
        asset_flow(&tx.asset_flows, ClearSigningAsset::Xch)
            .map_or(0, |flow| flow.unaccounted_amount),
        u128::from(total_fee - reserved_fee)
    );
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

    assert_eq!(
        asset_flow(&tx.asset_flows, ClearSigningAsset::Xch)
            .map_or(0, |flow| flow.unaccounted_amount),
        0
    );
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
    assert!(!spend.revoked);
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];

    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1000);
    assert_eq!(child.memos.p2_puzzle_hash, bob_puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_melt() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let IssuedAsset { id, .. } = issue_asset(&mut sim, &mut ctx, &alice, AssetKind::Cat, 900)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, alice.p2_puzzle_hash, 900, Memos::None)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let cat = cat_asset_flow(&tx.asset_flows);

    assert_eq!(cat.input_amount, 900);
    assert_eq!(cat.output_amount, 900);
    assert_eq!(cat.unaccounted_amount, 0);

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        Bytes::default(),
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;
    let tail_spend = Spend::new(tail_puzzle, tail_solution);
    let supply_delta = Delta::new(0, 750);

    let mut spends = Spends::new(alice.p2_puzzle_hash);

    let actions = [Action::run_tail(id, tail_spend, supply_delta)];

    let mut deltas = Deltas::from_actions(&actions);
    deltas.set_needed(Id::Xch);
    alice.select_coins(&sim, &mut spends, &deltas)?;

    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let xch = xch_asset_flow(&tx.asset_flows);
    let cat = cat_asset_flow(&tx.asset_flows);

    assert_eq!(xch.input_amount, 100);
    assert_eq!(xch.output_amount, 850);
    assert_eq!(xch.issued_amount, 750);
    assert_eq!(xch.melted_amount, 0);
    assert_eq!(xch.unaccounted_amount, 0);

    assert_eq!(cat.input_amount, 900);
    assert_eq!(cat.output_amount, 150);
    assert_eq!(cat.issued_amount, 0);
    assert_eq!(cat.melted_amount, 750);
    assert_eq!(cat.unaccounted_amount, 0);

    Ok(())
}

#[rstest]
fn test_clear_signing_run_tail_increases_cat_supply() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2000)?;

    let IssuedAsset { id, .. } = issue_asset(&mut sim, &mut ctx, &alice, AssetKind::Cat, 900)?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(id, alice.p2_puzzle_hash, 900, Memos::None)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let cat = cat_asset_flow(&tx.asset_flows);

    assert_eq!(cat.input_amount, 900);
    assert_eq!(cat.output_amount, 900);
    assert_eq!(cat.unaccounted_amount, 0);

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        Bytes::default(),
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;
    let tail_spend = Spend::new(tail_puzzle, tail_solution);
    let supply_delta = Delta::new(750, 0);

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::run_tail(id, tail_spend, supply_delta)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let xch = xch_asset_flow(&tx.asset_flows);
    let cat = cat_asset_flow(&tx.asset_flows);

    assert_eq!(xch.input_amount, 1100);
    assert_eq!(xch.output_amount, 350);
    assert_eq!(xch.issued_amount, 0);
    assert_eq!(xch.melted_amount, 750);
    assert_eq!(xch.unaccounted_amount, 0);

    assert_eq!(cat.input_amount, 900);
    assert_eq!(cat.output_amount, 1650);
    assert_eq!(cat.issued_amount, 750);
    assert_eq!(cat.melted_amount, 0);
    assert_eq!(cat.unaccounted_amount, 0);

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

    let xch = xch_asset_flow(&tx.asset_flows);
    assert_eq!(xch.unaccounted_amount, 0);

    let nft = nft_asset_flow(&tx.asset_flows, child_nft);
    assert_eq!(nft.unaccounted_amount, 0);

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
        Bytes::default(),
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
    } = &issuance.kind
    else {
        panic!("wrong issuance kind");
    };

    assert_eq!(
        *singleton_struct_hash,
        SingletonStruct::new(alice.info.launcher_id)
            .tree_hash()
            .into()
    );
    assert_eq!(nonce, &Bytes::default());

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
        Bytes::default(),
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
    } = &issuance.kind
    else {
        panic!("wrong issuance kind");
    };

    assert_eq!(
        *singleton_struct_hash,
        SingletonStruct::new(alice.info.launcher_id)
            .tree_hash()
            .into()
    );
    assert_eq!(nonce, &Bytes::default());

    let cat_spend = tx
        .spends
        .iter()
        .find(|spend| matches!(spend.asset, ParsedAsset::Cat(_)))
        .expect("expected the eve cat spend to be verified");
    assert_eq!(cat_spend.asset.coin().coin_id(), issuance.coin_id);

    Ok(())
}

#[test]
fn test_clear_signing_minimum_extra_delta() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        Bytes::default(),
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;
    let asset_id: Bytes32 = ctx.tree_hash(tail_puzzle).into();

    let delegated_spend =
        ctx.delegated_spend(Conditions::new().run_cat_tail(tail_puzzle, tail_solution))?;
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

    // An untrusted transaction can claim to melt the most that an extra delta can represent
    eve.spend(
        &mut ctx,
        SingleCatSpend {
            p2_spend: delegated_spend,
            prev_coin_id: eve_coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: eve_coin.parent_coin_info,
                inner_puzzle_hash: delegated_puzzle_hash,
                amount: eve_coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: i64::MIN,
            revoke: false,
        },
    )?;

    spends
        .conditions
        .required
        .push(Condition::assert_concurrent_spend(eve_coin.coin_id()));

    let result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.issuances.len(), 1);
    assert_eq!(tx.issuances[0].extra_delta, i64::MIN);

    let melted = u128::from(i64::MIN.unsigned_abs());
    assert_eq!(cat_asset_flow(&tx.asset_flows).melted_amount, melted);
    assert_eq!(xch_asset_flow(&tx.asset_flows).issued_amount, melted);

    Ok(())
}

#[test]
fn test_clear_signing_unknown_tail_issuance() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let tail_puzzle = ctx.alloc(&clvm_quote!(()))?;
    let asset_id: Bytes32 = ctx.tree_hash(tail_puzzle).into();

    let delegated_spend =
        ctx.delegated_spend(Conditions::new().run_cat_tail(tail_puzzle, NodePtr::NIL))?;
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

    eve.spend(
        &mut ctx,
        SingleCatSpend {
            p2_spend: delegated_spend,
            prev_coin_id: eve_coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: eve_coin.parent_coin_info,
                inner_puzzle_hash: delegated_puzzle_hash,
                amount: eve_coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: -1000,
            revoke: false,
        },
    )?;

    spends
        .conditions
        .required
        .push(Condition::assert_concurrent_spend(eve_coin.coin_id()));

    let result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.issuances.len(), 1);
    assert_eq!(tx.issuances[0].kind, IssuanceKind::Unknown);
    assert_eq!(tx.issuances[0].extra_delta, -1000);
    assert!(!tx.issuances[0].verified);

    // The extra delta isn't verified, so it isn't counted as a melt.
    assert_eq!(cat_asset_flow(&tx.asset_flows).melted_amount, 0);
    assert_eq!(xch_asset_flow(&tx.asset_flows).issued_amount, 0);

    Ok(())
}

/// Sends 1000 mojos from the vault to an eve CAT whose TAIL is an everything with singleton TAIL for
/// `tail_launcher_id`, and whose p2 puzzle is a quoted list of conditions that runs it, then melts all of it. The vault
/// sends the eve CAT the extra delta message, which the test wallet only does by itself for the vault's own TAIL.
pub(super) fn melt_eve_cat(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    vault: &TestVault,
    tail_launcher_id: Bytes32,
) -> Result<TransactionData> {
    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        tail_launcher_id,
        Bytes::default(),
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        vault.info.custody_hash.into(),
    ))?;
    let asset_id: Bytes32 = ctx.tree_hash(tail_puzzle).into();

    let delegated_spend =
        ctx.delegated_spend(Conditions::new().run_cat_tail(tail_puzzle, tail_solution))?;
    let delegated_puzzle_hash = tree_hash(ctx, delegated_spend.puzzle).into();
    let eve_info = CatInfo::new(asset_id, None, delegated_puzzle_hash);

    let actions = [Action::send(
        Id::Xch,
        eve_info.puzzle_hash().into(),
        1000,
        Memos::None,
    )];

    let mut spends = Spends::new(vault.p2_puzzle_hash);
    vault.select_coins(sim, &mut spends, &Deltas::from_actions(&actions))?;

    let input_coin_id = spends.xch.items[0].asset.coin_id();
    let eve_coin = Coin::new(input_coin_id, eve_info.puzzle_hash().into(), 1000);
    let eve = Cat::new(eve_coin, None, eve_info);

    eve.spend(
        ctx,
        SingleCatSpend {
            p2_spend: delegated_spend,
            prev_coin_id: eve_coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: eve_coin.parent_coin_info,
                inner_puzzle_hash: delegated_puzzle_hash,
                amount: eve_coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: -1000,
            revoke: false,
        },
    )?;

    let mut vault_conditions = Conditions::new();

    if tail_launcher_id != vault.info.launcher_id {
        let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
            | MessageFlags::COIN.encode(MessageSide::Receiver);
        let eve_coin_id = ctx.alloc(&eve_coin.coin_id())?;
        vault_conditions =
            vault_conditions.send_message(mode, get_extra_delta_message(-1000), vec![eve_coin_id]);
    }

    vault.partial_custom_spend(sim, ctx, &actions, spends, vault_conditions)
}

#[test]
fn test_clear_signing_verified_issuance() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    let result = melt_eve_cat(&mut sim, &mut ctx, &alice, alice.info.launcher_id)?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.issuances.len(), 1);
    assert!(tx.issuances[0].verified);
    assert_eq!(cat_asset_flow(&tx.asset_flows).melted_amount, 1000);
    assert_eq!(xch_asset_flow(&tx.asset_flows).issued_amount, 1000);

    Ok(())
}

#[test]
fn test_clear_signing_issuance_of_another_singleton() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;

    // The TAIL is authorized by a different singleton, so the vault's message to the eve CAT doesn't authorize the
    // extra delta. The eve CAT's p2 puzzle could receive the message itself, so the melt isn't guaranteed.
    let result = melt_eve_cat(&mut sim, &mut ctx, &alice, Bytes32::new([7; 32]))?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;

    assert!(matches!(
        parse_vault_transaction(
            reveals,
            &mut ctx,
            alice.info.launcher_id,
            result.delegated_spend,
        ),
        Err(DriverError::UnmatchedVaultMessage)
    ));

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
    assert_eq!(child.asset, ParsedAsset::Bulletin(bulletin.clone()));
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

    let actions = [Action::send(id, alice.p2_puzzle_hash, 1, Memos::None)];
    let mut spends = Spends::new(bob.p2_puzzle_hash);
    bob.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let result = bob.custom_spend(
        &mut sim,
        &mut ctx,
        &actions,
        spends,
        Conditions::new().assert_seconds_absolute(clawback.seconds),
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

#[test]
fn test_clear_signing_clawback_to_settlement_payment() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;

    let clawback = ClawbackV2::new(
        alice.p2_puzzle_hash,
        SETTLEMENT_PAYMENT_HASH.into(),
        1000,
        1,
        false,
    );
    let memos = ctx.memos(&clvm_list!(
        Bytes32::from(SETTLEMENT_PAYMENT_HASH),
        clawback.memo()
    ))?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let child = &tx.spends[0].children[0];
    assert_eq!(child.memos.clawback, Some(clawback));
    assert_eq!(child.transfer_type, TransferType::Sent);
    assert_eq!(xch_asset_flow(&tx.asset_flows).output_amount, 1);
    assert_eq!(xch_asset_flow(&tx.asset_flows).unaccounted_amount, 0);

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
#[case::expiry_asserted(Some(1000))]
#[case::before_expiry_asserted(Some(999))]
#[case::expiry_unasserted(None)]
fn test_clear_signing_push_through(#[case] earliest_time: Option<u64>) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];

    sim.set_next_timestamp(2000)?;

    // The vault only pays the fee, and asserts the push-through spend that nobody needs to sign.
    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let mut vault_conditions = Conditions::new().assert_concurrent_spend(clawback_coin.coin_id());

    if let Some(earliest_time) = earliest_time {
        vault_conditions = vault_conditions.assert_seconds_absolute(earliest_time);
    }

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.reserved_fee, 1);

    // Unless the vault asserts that the clawback has expired, the clawback coin could be clawed back to the sender
    // instead, so the push-through can't be shown.
    if earliest_time.is_none_or(|time| time < clawback.seconds) {
        assert_eq!(tx.spends.len(), 1);
        assert!(tx.spends[0].clawback.is_none());
        return Ok(());
    }

    assert_eq!(tx.spends.len(), 2);

    let spend = &tx.spends[1];
    check_asset(&spend.asset, None, None, 1);
    assert_eq!(spend.asset.coin(), clawback_coin);
    assert_eq!(
        spend.clawback,
        Some(ClawbackInfo::new(clawback, ClawbackPath::PushThrough))
    );
    assert!(matches!(spend.custody, CustodyInfo::DelegatedConditions(_)));
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    check_asset(&child.asset, None, None, 1);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(child.transfer_type, TransferType::Sent);

    let xch = xch_asset_flow(&tx.asset_flows);
    assert_eq!(xch.unaccounted_amount, 0);

    Ok(())
}

#[test]
fn test_clear_signing_push_through_partially_expired() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let expired = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let unexpired = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 2000, 1, true);

    for clawback in [expired, unexpired] {
        let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;
        let _ = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
        )?;
    }

    let expired_coin = sim.unspent_coins(expired.tree_hash().into(), false)[0];
    let unexpired_coin = sim.unspent_coins(unexpired.tree_hash().into(), false)[0];

    sim.set_next_timestamp(3000)?;

    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(expired_coin.coin_id())
        .assert_concurrent_spend(unexpired_coin.coin_id())
        .assert_seconds_absolute(expired.seconds);

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    expired.push_through_coin_spend(&mut ctx, expired_coin)?;
    unexpired.push_through_coin_spend(&mut ctx, unexpired_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(expired);
    reveals.reveal_clawback(unexpired);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    // Only the clawback that the vault asserted the expiry of can be shown.
    assert_eq!(tx.spends.len(), 2);
    assert_eq!(tx.spends[1].asset.coin(), expired_coin);

    Ok(())
}

#[test]
fn test_clear_signing_push_through_replaceable_expiry() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];

    // The receiver path of this clawback claims to assert the expiry, but it could be replaced with the sender path.
    let receiver_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_seconds_absolute(clawback.seconds)
    ))?;
    let decoy = ClawbackV2::new(
        alice.p2_puzzle_hash,
        ctx.tree_hash(receiver_puzzle).into(),
        5000,
        1,
        false,
    );

    let actions = [Action::send(
        Id::Xch,
        decoy.tree_hash().into(),
        1,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let decoy_coin = Coin::new(
        spends.xch.items[0].asset.coin_id(),
        decoy.tree_hash().into(),
        1,
    );

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_concurrent_spend(decoy_coin.coin_id());

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let decoy_spend = decoy.receiver_spend(&mut ctx, Spend::new(receiver_puzzle, NodePtr::NIL))?;
    ctx.spend(decoy_coin, decoy_spend)?;
    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    reveals.reveal_clawback(decoy);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert!(
        tx.spends
            .iter()
            .all(|spend| ![clawback_coin, decoy_coin].contains(&spend.asset.coin()))
    );

    Ok(())
}

#[test]
fn test_clear_signing_push_through_ephemeral_clawback() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    sim.set_next_timestamp(2000)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let actions = [Action::send(
        Id::Xch,
        clawback.tree_hash().into(),
        1,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let clawback_coin = Coin::new(
        spends.xch.items[0].asset.coin_id(),
        clawback.tree_hash().into(),
        1,
    );

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_seconds_absolute(clawback.seconds);

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 2);
    assert_eq!(tx.spends[1].asset.coin(), clawback_coin);
    assert_eq!(tx.spends[1].children.len(), 1);
    assert_eq!(
        tx.spends[1].children[0].memos.p2_puzzle_hash,
        bob.p2_puzzle_hash
    );

    Ok(())
}

#[test]
fn test_clear_signing_push_through_spent_child() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    // The receiver's puzzle sends the clawback's value on to Bob.
    let receiver_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
        bob.p2_puzzle_hash,
        1,
        Memos::None
    )))?;
    let receiver_puzzle_hash: Bytes32 = ctx.tree_hash(receiver_puzzle).into();

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, receiver_puzzle_hash, 1000, 1, false);
    let memos = ctx.memos(&clvm_list!(receiver_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];
    let child_coin = Coin::new(clawback_coin.coin_id(), receiver_puzzle_hash, 1);

    sim.set_next_timestamp(2000)?;

    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_concurrent_spend(child_coin.coin_id())
        .assert_seconds_absolute(clawback.seconds);

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    ctx.spend(child_coin, Spend::new(receiver_puzzle, NodePtr::NIL))?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 3);
    assert_eq!(tx.spends[1].asset.coin(), clawback_coin);
    assert_eq!(tx.spends[2].asset.coin(), child_coin);
    assert_eq!(
        tx.spends[2].children[0].memos.p2_puzzle_hash,
        bob.p2_puzzle_hash
    );

    Ok(())
}

#[test]
fn test_clear_signing_push_through_amount_mismatch() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    // The clawback's amount isn't committed to by the coin, which has more value than the push-through outputs.
    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 2, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];

    sim.set_next_timestamp(2000)?;

    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_seconds_absolute(clawback.seconds);

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 1);
    assert!(tx.spends[0].clawback.is_none());

    Ok(())
}

#[test]
fn test_clear_signing_push_through_wrong_puzzle() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];

    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_seconds_absolute(clawback.seconds);

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    // The merkle proof is for the push-through path, but the revealed puzzle isn't.
    let merkle_tree = clawback.merkle_tree();
    let merkle_proof = merkle_tree
        .proof(clawback.push_through_path_hash())
        .expect("merkle proof should exist");
    let puzzle = ctx.alloc(&clvm_quote!(Conditions::new()))?;
    let spend = clawback.into_1_of_n().construct_spend(
        &mut ctx,
        P2OneOfManySolution::new(merkle_proof, puzzle, NodePtr::NIL),
    )?;
    ctx.spend(clawback_coin, spend)?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(tx, Err(DriverError::InvalidMerkleProof)));

    Ok(())
}

#[test]
fn test_clear_signing_unrelated_unparseable_spend() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;
    let coin = vault_coin(&alice, 1000);

    // A spend of a coin owned by another vault, whose delegated spend isn't quoted conditions.
    let TestP2Puzzle::P2Singleton(bob_p2_singleton) = bob.p2_puzzles[&bob.p2_puzzle_hash.into()]
    else {
        panic!("expected p2 singleton");
    };
    let bob_coin = Coin::new(Bytes32::new([1; 32]), bob.p2_puzzle_hash, 1000);
    let bob_spend = bob_p2_singleton.spend(
        &mut ctx,
        bob.info.custody_hash.into(),
        1,
        Spend::new(NodePtr::NIL, NodePtr::NIL),
    )?;
    ctx.spend(bob_coin, bob_spend)?;
    let bob_coin_spends = ctx.take();

    let delegated_spend = ctx.delegated_spend(Conditions::new().create_coin(
        alice.p2_puzzle_hash,
        1000,
        Memos::None,
    ))?;
    let tx = parse_linked_spend(
        &mut ctx,
        &alice,
        coin,
        delegated_spend,
        Conditions::new(),
        &bob_coin_spends,
    )??;

    assert_eq!(tx.spends.len(), 1);
    assert_eq!(tx.spends[0].asset.coin(), coin);

    Ok(())
}

/// Mints a vault and locks one of its coins in a clawback to Bob.
fn push_through_setup(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
) -> Result<(TestVault, ClawbackV2, Coin)> {
    let alice = TestVault::mint(sim, ctx, 3)?;
    let bob = TestVault::mint(sim, ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, true);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        sim,
        ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    let clawback_coin = sim.unspent_coins(clawback.tree_hash().into(), false)[0];

    Ok((alice, clawback, clawback_coin))
}

/// Creates a clawback whose receiver is a quoted puzzle, so that its spend isn't bound to a vault message.
fn decoy_clawback(
    ctx: &mut SpendContext,
    conditions: Conditions,
    seconds: u64,
) -> Result<(ClawbackV2, NodePtr)> {
    let puzzle = ctx.alloc(&clvm_quote!(conditions))?;
    let sender_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()))?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(sender_puzzle).into(),
        ctx.tree_hash(puzzle).into(),
        seconds,
        1,
        false,
    );
    Ok((clawback, puzzle))
}

#[test]
fn test_clear_signing_replaceable_spend_chain_asserts_expiry() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let (alice, clawback, clawback_coin) = push_through_setup(&mut sim, &mut ctx)?;

    // A coin that really would assert the expiry, but only a replaceable spend asserts that it's spent.
    let expiry_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_seconds_absolute(clawback.seconds)
    ))?;
    let expiry_puzzle_hash: Bytes32 = ctx.tree_hash(expiry_puzzle).into();

    let actions = [Action::send(Id::Xch, expiry_puzzle_hash, 1, Memos::None)];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let parent = spends.xch.items[0].asset.coin_id();
    let expiry_coin = Coin::new(parent, expiry_puzzle_hash, 1);

    let (decoy, decoy_puzzle) = decoy_clawback(
        &mut ctx,
        Conditions::new().assert_concurrent_spend(expiry_coin.coin_id()),
        5000,
    )?;

    // The decoy has to be a child of a verified spend, so it's created by a second output.
    let actions = [
        actions[0].clone(),
        Action::send(Id::Xch, decoy.tree_hash().into(), 1, Memos::None),
    ];
    let decoy_coin = Coin::new(parent, decoy.tree_hash().into(), 1);

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_concurrent_spend(decoy_coin.coin_id());

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let decoy_spend = decoy.receiver_spend(&mut ctx, Spend::new(decoy_puzzle, NodePtr::NIL))?;
    ctx.spend(decoy_coin, decoy_spend)?;
    ctx.spend(expiry_coin, Spend::new(expiry_puzzle, NodePtr::NIL))?;
    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    reveals.reveal_clawback(decoy);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert!(
        tx.spends
            .iter()
            .all(|spend| ![clawback_coin, decoy_coin, expiry_coin].contains(&spend.asset.coin()))
    );

    Ok(())
}

#[test]
fn test_clear_signing_replaceable_spend_asserts_spend() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let (alice, clawback, clawback_coin) = push_through_setup(&mut sim, &mut ctx)?;

    let (decoy, decoy_puzzle) = decoy_clawback(
        &mut ctx,
        Conditions::new()
            .assert_concurrent_spend(clawback_coin.coin_id())
            .reserve_fee(1),
        5000,
    )?;

    let actions = [Action::send(
        Id::Xch,
        decoy.tree_hash().into(),
        1,
        Memos::None,
    )];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let decoy_coin = Coin::new(
        spends.xch.items[0].asset.coin_id(),
        decoy.tree_hash().into(),
        1,
    );

    // The vault truthfully asserts the expiry, but it doesn't assert that the clawback is spent.
    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(decoy_coin.coin_id())
        .assert_seconds_absolute(clawback.seconds);

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let decoy_spend = decoy.receiver_spend(&mut ctx, Spend::new(decoy_puzzle, NodePtr::NIL))?;
    ctx.spend(decoy_coin, decoy_spend)?;
    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    reveals.reveal_clawback(decoy);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.reserved_fee, 0);
    assert!(
        tx.spends
            .iter()
            .all(|spend| spend.asset.coin() != clawback_coin)
    );

    Ok(())
}

#[test]
fn test_clear_signing_replaceable_spend_asserts_expiration() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;

    // The vault claws back a coin, which relies on the transaction expiring before the clawback does.
    let refund_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
        alice.p2_puzzle_hash,
        1,
        Memos::None
    )))?;
    let (clawback, _) = decoy_clawback(&mut ctx, Conditions::new(), 1000)?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(refund_puzzle).into(),
        clawback.receiver_puzzle_hash,
        clawback.seconds,
        clawback.amount,
        clawback.hinted,
    );

    // Only a replaceable spend says that the transaction expires in time.
    let (decoy, decoy_puzzle) = decoy_clawback(
        &mut ctx,
        Conditions::new().assert_before_seconds_absolute(500),
        5000,
    )?;

    let actions = [
        Action::send(Id::Xch, clawback.tree_hash().into(), 1, Memos::None),
        Action::send(Id::Xch, decoy.tree_hash().into(), 1, Memos::None),
    ];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let parent = spends.xch.items[0].asset.coin_id();
    let clawback_coin = Coin::new(parent, clawback.tree_hash().into(), 1);
    let decoy_coin = Coin::new(parent, decoy.tree_hash().into(), 1);

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_concurrent_spend(decoy_coin.coin_id());

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let claw_back_spend =
        clawback.sender_spend(&mut ctx, Spend::new(refund_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, claw_back_spend)?;
    let decoy_spend = decoy.receiver_spend(&mut ctx, Spend::new(decoy_puzzle, NodePtr::NIL))?;
    ctx.spend(decoy_coin, decoy_spend)?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    reveals.reveal_clawback(decoy);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(tx, Err(DriverError::UnguaranteedClawBack)));

    Ok(())
}

#[rstest]
#[case::expiry_asserted(Some(1000))]
#[case::before_expiry_asserted(Some(999))]
#[case::expiry_unasserted(None)]
fn test_clear_signing_receiver_path_without_message(
    #[case] earliest_time: Option<u64>,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let (clawback, receiver_puzzle) = decoy_clawback(
        &mut ctx,
        Conditions::new().create_coin(bob.p2_puzzle_hash, 1, Memos::None),
        1000,
    )?;

    let actions = [Action::send(
        Id::Xch,
        clawback.tree_hash().into(),
        1,
        Memos::None,
    )];

    sim.set_next_timestamp(2000)?;

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let clawback_coin = Coin::new(
        spends.xch.items[0].asset.coin_id(),
        clawback.tree_hash().into(),
        1,
    );

    let mut vault_conditions = Conditions::new().assert_concurrent_spend(clawback_coin.coin_id());

    if let Some(earliest_time) = earliest_time {
        vault_conditions = vault_conditions.assert_seconds_absolute(earliest_time);
    }

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let spend = clawback.receiver_spend(&mut ctx, Spend::new(receiver_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, spend)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    let shown = tx
        .spends
        .iter()
        .any(|spend| spend.asset.coin() == clawback_coin);

    // Unless the vault asserts that the clawback has expired, the receiver path could be replaced with the sender path.
    assert_eq!(shown, earliest_time == Some(clawback.seconds));

    Ok(())
}

#[rstest]
#[case::expiry_unasserted(false)]
#[case::expiry_asserted_by_clawback_spend(true)]
fn test_clear_signing_receiver_path_requires_vault_expiry(
    #[case] spend_asserts_expiry: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let mut bob = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let clawback = ClawbackV2::new(alice.p2_puzzle_hash, bob.p2_puzzle_hash, 1000, 1, false);
    let memos = ctx.memos(&clvm_list!(bob.p2_puzzle_hash, clawback.memo()))?;

    let _ = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::send(Id::Xch, clawback.tree_hash().into(), 1, memos)],
    )?;

    sim.set_next_timestamp(2000)?;

    bob.p2_puzzles
        .insert(clawback.tree_hash(), TestP2Puzzle::Clawback(clawback));

    let actions = [Action::send(Id::Xch, alice.p2_puzzle_hash, 1, Memos::None)];
    let mut spends = Spends::new(bob.p2_puzzle_hash);
    bob.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    // The sender path could consume the vault's message instead, so the clawback spend can't vouch for itself.
    if spend_asserts_expiry {
        let SpendKind::Conditions(spend) = &mut spends.xch.items[0].kind else {
            unreachable!("the clawback is spent with conditions");
        };
        spend.add_conditions(Conditions::new().assert_seconds_absolute(clawback.seconds));
    }

    let result = bob.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        bob.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(tx, Err(DriverError::UnguaranteedClawbackExpiry)));

    Ok(())
}

#[test]
fn test_clear_signing_sender_path_cant_assert_its_expiration() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;

    // This coin really would expire the transaction, but only the sender path asserts that it's spent.
    let expiration_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_before_seconds_absolute(1000)
    ))?;
    let expiration_puzzle_hash: Bytes32 = ctx.tree_hash(expiration_puzzle).into();

    let actions = [Action::send(
        Id::Xch,
        expiration_puzzle_hash,
        1,
        Memos::None,
    )];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let parent = spends.xch.items[0].asset.coin_id();
    let expiration_coin = Coin::new(parent, expiration_puzzle_hash, 1);

    let sender_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_concurrent_spend(expiration_coin.coin_id())
    ))?;
    let (receiver, _) = decoy_clawback(&mut ctx, Conditions::new(), 1000)?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(sender_puzzle).into(),
        receiver.receiver_puzzle_hash,
        1000,
        1,
        false,
    );

    // The clawback is created last, so that it's spent first by the traversal.
    let actions = [
        actions[0].clone(),
        Action::send(Id::Xch, clawback.tree_hash().into(), 1, Memos::None),
    ];
    let clawback_coin = Coin::new(parent, clawback.tree_hash().into(), 1);

    let vault_conditions = Conditions::new().assert_concurrent_spend(clawback_coin.coin_id());

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let spend = clawback.sender_spend(&mut ctx, Spend::new(sender_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, spend)?;
    ctx.spend(expiration_coin, Spend::new(expiration_puzzle, NodePtr::NIL))?;
    result.spend_bundle.coin_spends.extend(ctx.take());

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    assert!(matches!(tx, Err(DriverError::UnguaranteedClawBack)));

    Ok(())
}

#[test]
fn test_clear_signing_sender_path_facts_with_vault_expiration() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;

    let sender_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().reserve_fee(1)))?;
    let (receiver, _) = decoy_clawback(&mut ctx, Conditions::new(), 1000)?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(sender_puzzle).into(),
        receiver.receiver_puzzle_hash,
        1000,
        1,
        false,
    );

    let actions = [Action::send(
        Id::Xch,
        clawback.tree_hash().into(),
        1,
        Memos::None,
    )];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let clawback_coin = Coin::new(
        spends.xch.items[0].asset.coin_id(),
        clawback.tree_hash().into(),
        1,
    );

    // The vault rules out the other paths, so the sender path's conditions are guaranteed.
    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_before_seconds_absolute(clawback.seconds);

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let spend = clawback.sender_spend(&mut ctx, Spend::new(sender_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, spend)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.reserved_fee, 1);

    Ok(())
}

#[rstest]
#[test]
fn test_clear_signing_sender_path_facts_with_later_expiration(
    #[values(false, true)] clawback_first: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 3)?;

    // The expiration coin's conditions are committed to by its puzzle hash, but it's only verified once the
    // traversal reaches it as a child, which can be after the sender path is verified.
    let expiration_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_before_seconds_absolute(1000)
    ))?;
    let expiration_puzzle_hash: Bytes32 = ctx.tree_hash(expiration_puzzle).into();

    // Only the sender path asserts that this coin is spent.
    let asserted_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()))?;
    let asserted_puzzle_hash: Bytes32 = ctx.tree_hash(asserted_puzzle).into();

    let actions = [Action::send(Id::Xch, asserted_puzzle_hash, 1, Memos::None)];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let parent = spends.xch.items[0].asset.coin_id();
    let asserted_coin = Coin::new(parent, asserted_puzzle_hash, 1);

    let sender_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new()
            .reserve_fee(1)
            .assert_concurrent_spend(asserted_coin.coin_id())
    ))?;
    let (receiver, _) = decoy_clawback(&mut ctx, Conditions::new(), 1000)?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(sender_puzzle).into(),
        receiver.receiver_puzzle_hash,
        1000,
        1,
        false,
    );

    let expiration_action = Action::send(Id::Xch, expiration_puzzle_hash, 1, Memos::None);
    let clawback_action = Action::send(Id::Xch, clawback.tree_hash().into(), 1, Memos::None);

    // The last child created is the first one spent by the traversal, so the asserted coin is reached before the
    // sender path spend is trusted.
    let [expiration_action, clawback_action] = if clawback_first {
        [expiration_action, clawback_action]
    } else {
        [clawback_action, expiration_action]
    };
    let actions = [expiration_action, clawback_action, actions[0].clone()];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    assert_eq!(spends.xch.items[0].asset.coin_id(), parent);
    let expiration_coin = Coin::new(parent, expiration_puzzle_hash, 1);
    let clawback_coin = Coin::new(parent, clawback.tree_hash().into(), 1);

    let vault_conditions = Conditions::new()
        .assert_concurrent_spend(clawback_coin.coin_id())
        .assert_concurrent_spend(expiration_coin.coin_id());

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let spend = clawback.sender_spend(&mut ctx, Spend::new(sender_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, spend)?;
    ctx.spend(expiration_coin, Spend::new(expiration_puzzle, NodePtr::NIL))?;
    ctx.spend(asserted_coin, Spend::new(asserted_puzzle, NodePtr::NIL))?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    // The vault-asserted expiration rules out the other paths, regardless of when it's verified.
    assert_eq!(tx.reserved_fee, 1);
    assert!(
        tx.spends
            .iter()
            .any(|spend| spend.asset.coin().coin_id() == asserted_coin.coin_id())
    );

    Ok(())
}

#[rstest]
#[test]
fn test_clear_signing_asserted_after_visited(
    #[values(false, true)] asserted_first: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let asserted_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()))?;
    let asserted_puzzle_hash: Bytes32 = ctx.tree_hash(asserted_puzzle).into();

    let actions = [Action::send(Id::Xch, asserted_puzzle_hash, 1, Memos::None)];
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    let parent = spends.xch.items[0].asset.coin_id();
    let asserted_coin = Coin::new(parent, asserted_puzzle_hash, 1);

    // Only this coin asserts that the other one is spent, and only the vault asserts that this one is spent.
    let asserting_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_concurrent_spend(asserted_coin.coin_id())
    ))?;
    let asserting_puzzle_hash: Bytes32 = ctx.tree_hash(asserting_puzzle).into();
    let asserting_coin = Coin::new(parent, asserting_puzzle_hash, 1);

    let asserting_action = Action::send(Id::Xch, asserting_puzzle_hash, 1, Memos::None);

    // The last child created is the first one visited by the traversal.
    let actions = if asserted_first {
        [asserting_action, actions[0].clone()]
    } else {
        [actions[0].clone(), asserting_action]
    };

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    assert_eq!(spends.xch.items[0].asset.coin_id(), parent);

    let vault_conditions = Conditions::new().assert_concurrent_spend(asserting_coin.coin_id());
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    ctx.spend(asserting_coin, Spend::new(asserting_puzzle, NodePtr::NIL))?;
    ctx.spend(asserted_coin, Spend::new(asserted_puzzle, NodePtr::NIL))?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert!(
        tx.spends
            .iter()
            .any(|spend| spend.asset.coin().coin_id() == asserted_coin.coin_id())
    );

    Ok(())
}

#[rstest]
#[test]
fn test_clear_signing_asserted_after_visited_claw_back_unguaranteed(
    #[values(false, true)] clawback_first: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let sender_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()))?;
    let (receiver, _) = decoy_clawback(&mut ctx, Conditions::new(), 1000)?;
    let clawback = ClawbackV2::new(
        ctx.tree_hash(sender_puzzle).into(),
        receiver.receiver_puzzle_hash,
        1000,
        1,
        false,
    );

    let clawback_action = Action::send(Id::Xch, clawback.tree_hash().into(), 1, Memos::None);
    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(
        &sim,
        &mut spends,
        &Deltas::from_actions(std::slice::from_ref(&clawback_action)),
    )?;
    let parent = spends.xch.items[0].asset.coin_id();
    let clawback_coin = Coin::new(parent, clawback.tree_hash().into(), 1);

    // The claw back is asserted, but nothing guarantees that the transaction expires before the clawback does.
    let asserting_puzzle = ctx.alloc(&clvm_quote!(
        Conditions::new().assert_concurrent_spend(clawback_coin.coin_id())
    ))?;
    let asserting_puzzle_hash: Bytes32 = ctx.tree_hash(asserting_puzzle).into();
    let asserting_coin = Coin::new(parent, asserting_puzzle_hash, 1);

    let asserting_action = Action::send(Id::Xch, asserting_puzzle_hash, 1, Memos::None);

    // The last child created is the first one visited by the traversal.
    let actions = if clawback_first {
        [asserting_action, clawback_action]
    } else {
        [clawback_action, asserting_action]
    };

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    assert_eq!(spends.xch.items[0].asset.coin_id(), parent);

    let vault_conditions = Conditions::new().assert_concurrent_spend(asserting_coin.coin_id());
    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    let spend = clawback.sender_spend(&mut ctx, Spend::new(sender_puzzle, NodePtr::NIL))?;
    ctx.spend(clawback_coin, spend)?;
    ctx.spend(asserting_coin, Spend::new(asserting_puzzle, NodePtr::NIL))?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    );

    // This doesn't depend on whether the clawback coin is visited before it's asserted.
    assert!(matches!(tx, Err(DriverError::UnguaranteedClawBack)));

    Ok(())
}

#[test]
fn test_clear_signing_push_through_message_bound_expiry() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let (alice, clawback, clawback_coin) = push_through_setup(&mut sim, &mut ctx)?;

    sim.set_next_timestamp(2000)?;

    let actions = [Action::Fee(FeeAction {
        amount: 1,
        reserved: true,
    })];

    let vault_conditions = Conditions::new().assert_concurrent_spend(clawback_coin.coin_id());

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    // The expiry is asserted by a spend that's bound to a vault message, rather than the vault itself.
    let SpendKind::Conditions(spend) = &mut spends.xch.items[0].kind else {
        unreachable!("the vault's coin is spent with conditions");
    };
    spend.add_conditions(Conditions::new().assert_seconds_absolute(clawback.seconds));

    let mut result =
        alice.partial_custom_spend(&mut sim, &mut ctx, &actions, spends, vault_conditions)?;

    clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;
    result.spend_bundle.coin_spends.extend(ctx.take());
    sim.new_transaction(result.spend_bundle.clone())?;

    let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
    reveals.reveal_clawback(clawback);
    let tx = parse_vault_transaction(
        reveals,
        &mut ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?;

    assert_eq!(tx.spends.len(), 2);
    assert_eq!(tx.spends[1].asset.coin(), clawback_coin);

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

#[test]
fn test_clear_signing_revocable_cat_child_uses_unwrapped_p2_puzzle_hash() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let child_p2_puzzle_hash = SETTLEMENT_PAYMENT_HASH.into();

    let tail_puzzle = ctx.curry(EverythingWithSingletonTailArgs::new(
        alice.info.launcher_id,
        Bytes::default(),
    ))?;
    let tail_solution = ctx.alloc(&EverythingWithSingletonTailSolution::new(
        alice.info.custody_hash.into(),
    ))?;

    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::issue_cat(
            Spend::new(tail_puzzle, tail_solution),
            Some(alice.p2_puzzle_hash),
            1,
        )],
    )?;
    let cat = result.outputs.cats[0][0];

    let revocable_child_puzzle_hash =
        RevocationArgs::new(alice.p2_puzzle_hash, child_p2_puzzle_hash)
            .curry_tree_hash()
            .into();
    let conditions = Conditions::new().create_coin(
        revocable_child_puzzle_hash,
        1,
        ctx.hint(child_p2_puzzle_hash)?,
    );
    let delegated_spend = ctx.delegated_spend(conditions)?;
    let delegated_puzzle_hash = ctx.tree_hash(delegated_spend.puzzle);

    let TestP2Puzzle::P2Singleton(p2_singleton) = alice.p2_puzzles[&alice.p2_puzzle_hash.into()]
    else {
        panic!("expected p2 singleton");
    };
    let p2_spend =
        p2_singleton.spend(&mut ctx, alice.info.custody_hash.into(), 1, delegated_spend)?;

    Cat::spend_all(&mut ctx, &[CatSpend::revoke(cat, p2_spend)])?;

    let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
        | MessageFlags::COIN.encode(MessageSide::Receiver);
    let coin_id = ctx.alloc(&cat.coin.coin_id())?;
    let vault_conditions =
        Conditions::new().send_message(mode, delegated_puzzle_hash.to_vec().into(), vec![coin_id]);

    let result = alice.custom_spend(
        &mut sim,
        &mut ctx,
        &[],
        Spends::new(alice.p2_puzzle_hash),
        vault_conditions,
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
    check_asset(
        &spend.asset,
        Some(cat.info.asset_id),
        Some(alice.p2_puzzle_hash),
        1,
    );
    assert_eq!(spend.children.len(), 1);
    assert!(spend.revoked);

    let child = &spend.children[0];
    let expected_child_info = CatInfo::new(
        cat.info.asset_id,
        Some(alice.p2_puzzle_hash),
        child_p2_puzzle_hash,
    );

    check_asset(
        &child.asset,
        Some(cat.info.asset_id),
        Some(alice.p2_puzzle_hash),
        1,
    );
    assert_eq!(
        child.asset.coin().puzzle_hash,
        expected_child_info.puzzle_hash().into()
    );
    assert_eq!(child.memos.p2_puzzle_hash, child_p2_puzzle_hash);
    assert_eq!(child.transfer_type, TransferType::Offered);

    Ok(())
}

#[rstest]
fn test_clear_signing_create_p2_conditions_or_singleton(
    #[values(false, true)] revealed_up_front: bool,
    #[values(
        P2ConditionsOrSingletonVersion::Legacy,
        P2ConditionsOrSingletonVersion::FixedPuzzleMember
    )]
    version: P2ConditionsOrSingletonVersion,
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
        version,
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
                received_payments: vec![],
                external_payments: vec![],
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
fn test_clear_signing_spend_p2_conditions_or_singleton_fixed(
    #[values(
        P2ConditionsOrSingletonVersion::Legacy,
        P2ConditionsOrSingletonVersion::FixedPuzzleMember
    )]
    version: P2ConditionsOrSingletonVersion,
) -> Result<()> {
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
        version,
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

    assert_eq!(
        asset_flow(&tx.asset_flows, ClearSigningAsset::Xch).map(|flow| flow.unaccounted_amount),
        Some(1)
    );
    assert_eq!(tx.reserved_fee, 0);

    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 0);

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 0);

    let payment = &tx.asserted_payments[0];
    assert_eq!(payment.asset, ClearSigningAsset::Xch);
    assert_eq!(payment.notarized_payment.payments.len(), 1);

    let payment = &payment.notarized_payment.payments[0];
    assert_eq!(payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(payment.amount, 750);

    Ok(())
}

#[rstest]
fn test_clear_signing_spend_p2_conditions_or_singleton_vault(
    #[values(
        P2ConditionsOrSingletonVersion::Legacy,
        P2ConditionsOrSingletonVersion::FixedPuzzleMember
    )]
    version: P2ConditionsOrSingletonVersion,
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
        version,
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
        vec![requested_payment.clone(), requested_payment],
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

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 0);

    let payment = &tx.asserted_payments[0];
    assert_eq!(payment.asset, ClearSigningAsset::Xch);
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

    let xch_flow = asset_flow(&tx.asset_flows, ClearSigningAsset::Xch).unwrap();
    assert_eq!(xch_flow.paid_amount, 1000);
    assert_eq!(xch_flow.unaccounted_amount, 0);
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.asserted_payments.len(), 2);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 1);

    let xch_payment = &tx.asserted_payments[0];
    assert_eq!(xch_payment.asset, ClearSigningAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    let cat_payment = &tx.asserted_payments[1];
    assert_eq!(
        cat_payment.asset,
        ClearSigningAsset::Cat {
            asset_id: asset_id.unwrap(),
            hidden_puzzle_hash: None,
        }
    );
    assert_eq!(cat_payment.notarized_payment.payments.len(), 1);

    let cat_payment = &cat_payment.notarized_payment.payments[0];
    assert_eq!(cat_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(cat_payment.amount, 1000);

    assert_eq!(tx.external_payments[0].payment.amount, 1000);
    assert_eq!(tx.received_payments[0].payment.amount, 1000);

    Ok(())
}

#[rstest]
fn test_clear_signing_nft_offer() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 1030)?;

    // Mint an NFT
    let result = alice.spend(
        &mut sim,
        &mut ctx,
        &[Action::mint_nft(
            HashedPtr::NIL,
            NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
            alice.p2_puzzle_hash,
            300,
            1,
        )],
    )?;
    let nft = result.outputs.nfts[0];

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

    let royalty_payment = NotarizedPayment::new(
        nft.info.launcher_id,
        vec![Payment::new(
            nft.info.royalty_puzzle_hash,
            30,
            ctx.hint(nft.info.royalty_puzzle_hash)?,
        )],
    );

    let royalty_payment_hash = tree_hash_notarized_payment(&ctx, &royalty_payment);

    // Make the offer
    let actions = [
        Action::update_nft(
            Id::Existing(nft.info.launcher_id),
            vec![],
            Some(TransferNftById::new(
                None,
                vec![TradePrice::new(1000, SETTLEMENT_PAYMENT_HASH.into())],
            )),
        ),
        Action::send(
            Id::Existing(nft.info.launcher_id),
            SETTLEMENT_PAYMENT_HASH.into(),
            1,
            Memos::None,
        ),
    ];

    let mut spends = Spends::new(alice.p2_puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;
    spends
        .conditions
        .required
        .push(Condition::assert_puzzle_announcement(announcement_id(
            SETTLEMENT_PAYMENT_HASH.into(),
            notarized_payment_hash,
        )));
    spends
        .conditions
        .required
        .push(Condition::assert_puzzle_announcement(announcement_id(
            SETTLEMENT_PAYMENT_HASH.into(),
            royalty_payment_hash,
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
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 0);

    let xch_payment = &tx.asserted_payments[0];
    assert_eq!(xch_payment.asset, ClearSigningAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    // Take the offer
    let settlement_nft = result.outputs.nfts[0];
    let maker_bundle = result.spend_bundle;

    let actions = [
        Action::settle(Id::Xch, notarized_payment.clone()),
        Action::settle(Id::Xch, royalty_payment.clone()),
    ];

    let mut spends = Spends::new(bob.p2_puzzle_hash);
    spends.add(settlement_nft);
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

    let xch_flow = asset_flow(&tx.asset_flows, ClearSigningAsset::Xch).unwrap();
    assert_eq!(xch_flow.paid_amount, 1030);
    assert_eq!(xch_flow.unaccounted_amount, 0);
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.asserted_payments.len(), 3);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 2);

    let xch_payment = &tx.asserted_payments[0];
    assert_eq!(xch_payment.asset, ClearSigningAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    let xch_royalty_payment = &tx.asserted_payments[1];
    assert_eq!(xch_royalty_payment.asset, ClearSigningAsset::Xch);
    assert_eq!(
        xch_royalty_payment.notarized_payment.nonce,
        nft.info.launcher_id
    );

    let xch_royalty_payment = &xch_royalty_payment.notarized_payment.payments[0];
    assert_eq!(
        xch_royalty_payment.puzzle_hash,
        nft.info.royalty_puzzle_hash
    );
    assert_eq!(xch_royalty_payment.amount, 30);

    let nft_payment = &tx.asserted_payments[2];
    assert_eq!(
        nft_payment.asset,
        ClearSigningAsset::Nft {
            launcher_id: nft.info.launcher_id,
            metadata: nft.info.metadata,
            metadata_updater_puzzle_hash: nft.info.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: nft.info.royalty_puzzle_hash,
            royalty_basis_points: nft.info.royalty_basis_points,
        }
    );
    assert_eq!(nft_payment.notarized_payment.payments.len(), 1);

    let nft_payment = &nft_payment.notarized_payment.payments[0];
    assert_eq!(nft_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(nft_payment.amount, 1);

    assert_eq!(tx.external_payments[0].payment.amount, 1000);
    assert_eq!(tx.external_payments[0].royalty_basis_points, None);
    assert_eq!(tx.external_payments[1].payment.amount, 30);
    assert_eq!(
        tx.external_payments[1].royalty_basis_points,
        Some(nft.info.royalty_basis_points)
    );
    assert_eq!(tx.received_payments[0].payment.amount, 1);
    assert_eq!(tx.received_payments[0].royalty_basis_points, None);

    Ok(())
}

/// Reveals a requested payment to the vault from a settlement coin with the given puzzle, and has the vault assert
/// its announcement.
fn parse_asserted_settlement_payment(
    ctx: &mut SpendContext,
    vault: &TestVault,
    settlement_puzzle: NodePtr,
) -> Result<Result<VaultTransaction, DriverError>> {
    parse_asserted_settlement_payments(ctx, vault, &[settlement_puzzle])
}

/// Reveals the same requested payment to the vault for each settlement puzzle, and asserts each of their
/// announcements from the vault.
fn parse_asserted_settlement_payments(
    ctx: &mut SpendContext,
    vault: &TestVault,
    settlement_puzzles: &[NodePtr],
) -> Result<Result<VaultTransaction, DriverError>> {
    let notarized_payment = NotarizedPayment::new(
        Bytes32::new([9; 32]),
        vec![Payment::new(vault.p2_puzzle_hash, 1, Memos::None)],
    );
    let settlement_solution = ctx.alloc(&SettlementPaymentsSolution::new(vec![
        notarized_payment.clone(),
    ]))?;
    let payment_hash = tree_hash_notarized_payment(ctx, &notarized_payment);

    let mut coin_spends = Vec::new();
    let mut conditions = Conditions::new();

    for &settlement_puzzle in settlement_puzzles {
        let settlement_puzzle_hash: Bytes32 = ctx.tree_hash(settlement_puzzle).into();

        // Requested payments are revealed as coin spends with a default parent coin id.
        coin_spends.push(CoinSpend::new(
            Coin::new(Bytes32::default(), settlement_puzzle_hash, 1),
            ctx.serialize(&settlement_puzzle)?,
            ctx.serialize(&settlement_solution)?,
        ));

        conditions = conditions
            .assert_puzzle_announcement(announcement_id(settlement_puzzle_hash, payment_hash));
    }

    let delegated_spend = ctx.delegated_spend(conditions.create_coin(
        vault.info.custody_hash.into(),
        1,
        Memos::None,
    ))?;

    let reveals = Reveals::from_coin_spends(ctx, &coin_spends)?;

    Ok(parse_vault_transaction(
        reveals,
        ctx,
        vault.info.launcher_id,
        delegated_spend,
    ))
}

#[rstest]
fn test_clear_signing_asserted_nft_payment_with_owner(
    #[values(None, Some(Bytes32::new([8; 32])))] owner: Option<Bytes32>,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let settlement = ctx.alloc_mod::<SettlementPayment>()?;
    let info = NftInfo::new(
        Bytes32::new([1; 32]),
        HashedPtr::NIL,
        NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
        owner,
        Bytes32::new([2; 32]),
        300,
        SETTLEMENT_PAYMENT_HASH.into(),
    );
    let puzzle = info.into_layers(settlement).construct_puzzle(&mut ctx)?;

    // The settlement coin's puzzle hash commits to the NFT's owner, so the announcement is matched for the owner that
    // the requested payment is revealed with.
    let tx = parse_asserted_settlement_payment(&mut ctx, &alice, puzzle)??;

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(
        tx.asserted_payments[0].asset,
        ClearSigningAsset::Nft {
            launcher_id: info.launcher_id,
            metadata: info.metadata,
            metadata_updater_puzzle_hash: info.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: info.royalty_puzzle_hash,
            royalty_basis_points: info.royalty_basis_points,
        }
    );
    assert_eq!(tx.received_payments.len(), 1);

    Ok(())
}

#[test]
fn test_clear_signing_asserted_nft_payment_with_two_owners() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let settlement = ctx.alloc_mod::<SettlementPayment>()?;
    let puzzles = [None, Some(Bytes32::new([8; 32]))]
        .into_iter()
        .map(|owner| {
            NftInfo::new(
                Bytes32::new([1; 32]),
                HashedPtr::NIL,
                NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
                owner,
                Bytes32::new([2; 32]),
                300,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .into_layers(settlement)
            .construct_puzzle(&mut ctx)
        })
        .collect::<Result<Vec<_>, _>>()?;

    // The same payment is asserted for settlement coins of the NFT with two different owners. Only one coin of the NFT
    // can exist, so the payment is only reported once.
    let tx = parse_asserted_settlement_payments(&mut ctx, &alice, &puzzles)??;

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(tx.received_payments.len(), 1);

    Ok(())
}

#[test]
fn test_clear_signing_asserted_option_payment() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let settlement = ctx.alloc_mod::<SettlementPayment>()?;
    let info = OptionInfo::new(
        Bytes32::new([1; 32]),
        Bytes32::new([2; 32]),
        Bytes32::new([3; 32]),
        SETTLEMENT_PAYMENT_HASH.into(),
    );
    let puzzle = info.into_layers(settlement).construct_puzzle(&mut ctx)?;

    // Option contracts can't be represented as a clear signing asset, so an asserted payment of one can't be reported,
    // and isn't silently left out either.
    assert!(matches!(
        parse_asserted_settlement_payment(&mut ctx, &alice, puzzle)?,
        Err(DriverError::UnsupportedAssertedOptionPayment)
    ));

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

    assert_eq!(tx.asserted_payments.len(), 0);
    assert_eq!(tx.received_payments.len(), 0);
    assert_eq!(tx.external_payments.len(), 0);

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

    let cat_asset = ClearSigningAsset::Cat {
        asset_id: asset_id.unwrap(),
        hidden_puzzle_hash: None,
    };
    assert_eq!(
        asset_flow(&tx.asset_flows, cat_asset).map(|flow| flow.unaccounted_amount),
        Some(1)
    );
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 0);

    assert_eq!(tx.asserted_payments.len(), 1);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 0);

    let cat_payment = &tx.asserted_payments[0];
    assert_eq!(
        cat_payment.asset,
        ClearSigningAsset::Cat {
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
        P2ConditionsOrSingletonVersion::FixedPuzzleMember,
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

    assert_eq!(tx.asserted_payments.len(), 0);
    assert_eq!(tx.received_payments.len(), 0);
    assert_eq!(tx.external_payments.len(), 0);

    assert_eq!(tx.linked_offer.as_ref().unwrap().reserved_fee, 0);
    assert_eq!(tx.linked_offer.as_ref().unwrap().received_payments.len(), 1);
    assert_eq!(
        tx.linked_offer.as_ref().unwrap().received_payments[0].asset,
        ClearSigningAsset::Xch
    );
    assert_eq!(
        tx.linked_offer.as_ref().unwrap().received_payments[0]
            .payment
            .amount,
        1000
    );
    assert_eq!(
        tx.linked_offer.as_ref().unwrap().received_payments[0]
            .payment
            .puzzle_hash,
        alice.p2_puzzle_hash
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

    let xch_flow = asset_flow(&tx.asset_flows, ClearSigningAsset::Xch).unwrap();
    assert_eq!(xch_flow.paid_amount, 1000);
    assert_eq!(xch_flow.unaccounted_amount, 0);
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(tx.spends.len(), 1);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];
    assert_eq!(child.asset.coin().amount, 0);
    assert_eq!(child.memos.p2_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    assert_eq!(tx.asserted_payments.len(), 2);
    assert_eq!(tx.received_payments.len(), 1);
    assert_eq!(tx.external_payments.len(), 1);

    let xch_payment = &tx.asserted_payments[0];
    assert_eq!(xch_payment.asset, ClearSigningAsset::Xch);
    assert_eq!(xch_payment.notarized_payment, notarized_payment);

    let cat_payment = &tx.asserted_payments[1];
    assert_eq!(
        cat_payment.asset,
        ClearSigningAsset::Cat {
            asset_id: asset_id.unwrap(),
            hidden_puzzle_hash: None,
        }
    );
    assert_eq!(cat_payment.notarized_payment.payments.len(), 1);

    let cat_payment = &cat_payment.notarized_payment.payments[0];
    assert_eq!(cat_payment.puzzle_hash, bob.p2_puzzle_hash);
    assert_eq!(cat_payment.amount, 750);

    assert_eq!(tx.external_payments[0].payment.amount, 1000);
    assert_eq!(tx.received_payments[0].payment.amount, 750);

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

/// A coin offered by a one-sided offer, which is spent as a settlement coin by the taker.
#[derive(Debug, Clone, Copy)]
enum OfferedCoin {
    Xch(Coin),
    Cat(Cat),
}

struct OneSidedOffer {
    offered: OfferedCoin,
    id: Id,
    asset: ClearSigningAsset,
    maker_bundle: SpendBundle,
}

/// Makes an offer of the amount of the asset from a new vault, which requests nothing in return.
fn make_one_sided_offer(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    asset_kind: AssetKind,
    amount: u64,
) -> Result<OneSidedOffer> {
    let alice = TestVault::mint(sim, ctx, 1000)?;

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(sim, ctx, &alice, asset_kind, 1000)?;

    let result = alice.partial_spend(
        sim,
        ctx,
        &[Action::send(
            id,
            SETTLEMENT_PAYMENT_HASH.into(),
            amount,
            Memos::None,
        )],
    )?;

    let (offered, asset) = if let Some(asset_id) = asset_id {
        let cat = result.outputs.cats[0]
            .iter()
            .find(|cat| cat.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into())
            .copied()
            .unwrap();

        (
            OfferedCoin::Cat(cat),
            ClearSigningAsset::Cat {
                asset_id,
                hidden_puzzle_hash,
            },
        )
    } else {
        let coin = result
            .outputs
            .xch
            .iter()
            .find(|coin| coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into())
            .copied()
            .unwrap();

        (OfferedCoin::Xch(coin), ClearSigningAsset::Xch)
    };

    Ok(OneSidedOffer {
        offered,
        id,
        asset,
        maker_bundle: result.spend_bundle,
    })
}

/// Takes the offer with the vault, selecting enough of its XCH for the fee, and makes sure that the
/// combined transaction is valid.
fn take_one_sided_offer(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    bob: &TestVault,
    offer: OneSidedOffer,
    actions: &[Action],
    fee: u64,
    return_unplaced_conditions: bool,
) -> Result<VaultTransaction> {
    let mut spends = Spends::new(bob.p2_puzzle_hash);

    match offer.offered {
        OfferedCoin::Xch(coin) => spends.add(coin),
        OfferedCoin::Cat(cat) => spends.add(cat),
    }

    spends.conditions.return_unplaced_conditions = return_unplaced_conditions;
    bob.select_coins(sim, &mut spends, &Deltas::from_actions(&[Action::fee(fee)]))?;

    let result = bob.partial_custom_spend(sim, ctx, actions, spends, Conditions::new())?;
    let reveals = Reveals::from_coin_spends(ctx, &result.spend_bundle.coin_spends)?;
    let tx = parse_vault_transaction(reveals, ctx, bob.info.launcher_id, result.delegated_spend)?;

    sim.new_transaction(SpendBundle::new(
        [
            offer.maker_bundle.coin_spends,
            result.spend_bundle.coin_spends,
        ]
        .concat(),
        offer.maker_bundle.aggregated_signature + &result.spend_bundle.aggregated_signature,
    ))?;

    Ok(tx)
}

fn received_payment_amount(tx: &VaultTransaction, asset: ClearSigningAsset) -> u128 {
    tx.received_payments
        .iter()
        .filter(|payment| payment.asset == asset)
        .map(|payment| u128::from(payment.payment.amount))
        .sum()
}

#[rstest]
fn test_clear_signing_vault_take_one_sided_offer(
    #[values(AssetKind::Xch, AssetKind::Cat)] asset_kind: AssetKind,
    #[values(0, 10)] fee: u64,
    #[values(false, true)] send_offered: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let offer = make_one_sided_offer(&mut sim, &mut ctx, asset_kind, 1)?;
    let bob = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let asset = offer.asset;

    // The taker either sends the offered amount to itself, or receives it as change
    let mut actions = Vec::new();

    if send_offered {
        let memos = if matches!(asset_kind, AssetKind::Xch) {
            Memos::None
        } else {
            ctx.hint(bob.p2_puzzle_hash)?
        };
        actions.push(Action::send(offer.id, bob.p2_puzzle_hash, 1, memos));
    }

    if fee > 0 {
        actions.push(Action::fee(fee));
    }

    let tx = take_one_sided_offer(&mut sim, &mut ctx, &bob, offer, &actions, fee, true)?;

    // Without a fee, the vault doesn't spend any coins, and asserts the payment itself.
    assert_eq!(tx.spends.len(), usize::from(fee > 0));
    assert_eq!(tx.reserved_fee, fee);

    // Nothing is unaccounted for, and the received amount is exactly what's asserted.
    for flow in &tx.asset_flows {
        assert_eq!(flow.unaccounted_amount, 0);
    }

    let flow = asset_flow(&tx.asset_flows, asset).unwrap();
    assert_eq!(flow.received_amount, 1);
    assert_eq!(received_payment_amount(&tx, asset), 1);
    assert_eq!(tx.received_payments.len(), 1);

    if fee > 0 {
        let xch = xch_asset_flow(&tx.asset_flows);
        assert_eq!(xch.input_amount, 1000);
        assert_eq!(xch.output_amount, 1000 - u128::from(fee));
    }

    Ok(())
}

fn bag_asset(asset_id: Option<Bytes32>, hidden_puzzle_hash: Option<Bytes32>) -> BagAsset {
    match (asset_id, hidden_puzzle_hash) {
        (None, _) => BagAsset::Xch,
        (Some(asset_id), None) => BagAsset::Cat { asset_id },
        (Some(asset_id), Some(hidden_puzzle_hash)) => BagAsset::RevocableCat {
            asset_id,
            hidden_puzzle_hash,
        },
    }
}

/// Sends the amount to the p2 puzzle hash from the vault, and parses the transaction with the bags revealed.
fn fund_bag(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    alice: &TestVault,
    id: Id,
    p2_puzzle_hash: Bytes32,
    amount: u64,
    bags: Vec<SecureTheBag>,
) -> Result<VaultTransaction> {
    let memos = ctx.hint(p2_puzzle_hash)?;

    let result = alice.spend(sim, ctx, &[Action::send(id, p2_puzzle_hash, amount, memos)])?;

    let mut reveals = Reveals::from_coin_spends(ctx, &result.spend_bundle.coin_spends)?;

    for bag in bags {
        reveals.reveal_bag(bag);
    }

    Ok(parse_vault_transaction(
        reveals,
        ctx,
        alice.info.launcher_id,
        result.delegated_spend,
    )?)
}

/// The child that the vault sent to the p2 puzzle hash, rather than the change.
fn sent_child(tx: &VaultTransaction, p2_puzzle_hash: Bytes32) -> &ParsedChild {
    let mut children = iter_final_children(&tx.spends).filter(|child| {
        child.memos.p2_puzzle_hash == p2_puzzle_hash
            || child
                .memos
                .clawback
                .is_some_and(|clawback| clawback.tree_hash() == p2_puzzle_hash.into())
    });
    let child = children.next().expect("missing child");
    assert!(children.next().is_none());
    child
}

#[rstest]
fn test_clear_signing_secure_the_bag(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob: Bytes32 = "bob".tree_hash().into();
    let carol: Bytes32 = "carol".tree_hash().into();
    let dave: Bytes32 = "dave".tree_hash().into();

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1000)?;

    let hinted = asset_id.is_some();

    // We can claw back the payment to dave until it expires, and receive the one from bob once it expires
    let our_clawback = ClawbackV2::new(alice.p2_puzzle_hash, dave, 1000, 100, hinted);
    let their_clawback = ClawbackV2::new(bob, alice.p2_puzzle_hash, 1000, 50, hinted);

    let bag = SecureTheBag::build(
        BagFormat::Legacy {
            leaf_width: Some(2),
            sorted: true,
        },
        bag_asset(asset_id, hidden_puzzle_hash),
        [
            BagPayment::new(bob, 300),
            BagPayment::new(carol, 200),
            BagPayment::from_clawback(our_clawback),
            BagPayment::from_clawback(their_clawback),
            BagPayment::new(alice.p2_puzzle_hash, 250),
            BagPayment::new(BURN_PUZZLE_HASH, 50),
            BagPayment::new(SETTLEMENT_PAYMENT_HASH.into(), 50),
        ],
    )?;
    assert!(bag.depth() > 1);

    let tx = fund_bag(
        &mut sim,
        &mut ctx,
        &alice,
        id,
        bag.root_hash(),
        bag.total_amount(),
        vec![bag.clone()],
    )?;

    let child = sent_child(&tx, bag.root_hash());
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1000);
    assert_eq!(child.asset.coin().puzzle_hash, bag.root_puzzle_hash());

    let expected_payments = bag
        .payments()
        .iter()
        .map(|&payment| ParsedBagPayment {
            payment,
            transfer_type: if payment.puzzle_hash == BURN_PUZZLE_HASH {
                TransferType::Burned
            } else {
                TransferType::Sent
            },
        })
        .collect::<Vec<_>>();

    assert_eq!(
        child.transfer_type,
        TransferType::SecureTheBag(SecureTheBagInfo {
            format: bag.format(),
            payments: expected_payments,
        })
    );

    let TransferType::SecureTheBag(info) = &child.transfer_type else {
        unreachable!();
    };

    // The display can tell the payments back to the vault apart from the others
    let ours = info
        .payments
        .iter()
        .filter(|payment| tx.p2_puzzle_hashes.contains(&payment.payment.puzzle_hash))
        .map(|payment| payment.payment)
        .collect::<Vec<_>>();
    assert_eq!(ours.len(), 2);
    assert!(ours.contains(&BagPayment::new(alice.p2_puzzle_hash, 250)));
    assert!(ours.contains(&BagPayment::from_clawback(their_clawback)));

    let clawbacks = info
        .payments
        .iter()
        .filter_map(|payment| payment.payment.clawback)
        .collect::<Vec<_>>();
    assert_eq!(clawbacks.len(), 2);
    assert!(clawbacks.contains(&our_clawback));
    assert!(clawbacks.contains(&their_clawback));

    // The root is only counted once, rather than also counting its payments
    let flow = asset_flow(&tx.asset_flows, asset_from_bag(bag.asset())).unwrap();
    assert_eq!(flow.input_amount, 1000);
    assert_eq!(flow.output_amount, 1000);
    assert_eq!(flow.paid_amount, 0);
    assert_eq!(flow.unaccounted_amount, 0);
    assert_eq!(iter_final_children(&tx.spends).count(), 1);

    // The root can be unrolled to every payment
    let root = match &child.asset {
        ParsedAsset::Xch(coin) => BagCoin::Xch(*coin),
        ParsedAsset::Cat(cat) => BagCoin::Cat(*cat),
        _ => unreachable!(),
    };
    let coins = bag.unroll(&mut ctx, root)?;
    sim.spend_coins(ctx.take(), &[])?;

    assert_eq!(coins.len(), 7);
    for coin in coins {
        assert!(sim.coin_state(coin.coin().coin_id()).is_some());
    }

    Ok(())
}

fn asset_from_bag(asset: BagAsset) -> ClearSigningAsset {
    match asset {
        BagAsset::Xch => ClearSigningAsset::Xch,
        BagAsset::Cat { asset_id } => ClearSigningAsset::Cat {
            asset_id,
            hidden_puzzle_hash: None,
        },
        BagAsset::RevocableCat {
            asset_id,
            hidden_puzzle_hash,
        } => ClearSigningAsset::Cat {
            asset_id,
            hidden_puzzle_hash: Some(hidden_puzzle_hash),
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum BagMismatch {
    Unrevealed,
    WrongPayment,
    AmountTooLow,
    AmountTooHigh,
    WrongAsset,
    WrongAssetId,
    WrongHiddenPuzzleHash,
}

#[rstest]
fn test_clear_signing_secure_the_bag_mismatch(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
    #[values(
        BagMismatch::Unrevealed,
        BagMismatch::WrongPayment,
        BagMismatch::AmountTooLow,
        BagMismatch::AmountTooHigh,
        BagMismatch::WrongAsset,
        BagMismatch::WrongAssetId,
        BagMismatch::WrongHiddenPuzzleHash
    )]
    mismatch: BagMismatch,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1001)?;
    let bob: Bytes32 = "bob".tree_hash().into();
    let carol: Bytes32 = "carol".tree_hash().into();

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1001)?;

    let asset = bag_asset(asset_id, hidden_puzzle_hash);
    let other = Bytes32::new([1; 32]);

    let bag = SecureTheBag::build(
        BagFormat::legacy(),
        asset,
        [BagPayment::new(bob, 600), BagPayment::new(carol, 400)],
    )?;

    let revealed_asset = match mismatch {
        BagMismatch::WrongAsset => match asset {
            BagAsset::Xch => BagAsset::Cat { asset_id: other },
            BagAsset::Cat { .. } => BagAsset::Xch,
            BagAsset::RevocableCat { asset_id, .. } => BagAsset::Cat { asset_id },
        },
        BagMismatch::WrongAssetId => match asset {
            BagAsset::Xch | BagAsset::Cat { .. } => BagAsset::Cat { asset_id: other },
            BagAsset::RevocableCat {
                hidden_puzzle_hash, ..
            } => BagAsset::RevocableCat {
                asset_id: other,
                hidden_puzzle_hash,
            },
        },
        BagMismatch::WrongHiddenPuzzleHash => match asset {
            BagAsset::Xch => BagAsset::RevocableCat {
                asset_id: other,
                hidden_puzzle_hash: other,
            },
            BagAsset::Cat { asset_id } | BagAsset::RevocableCat { asset_id, .. } => {
                BagAsset::RevocableCat {
                    asset_id,
                    hidden_puzzle_hash: other,
                }
            }
        },
        _ => asset,
    };

    // The same total, but paid to bob and carol differently, which changes the root
    let revealed_payments = match mismatch {
        BagMismatch::WrongPayment => vec![BagPayment::new(bob, 601), BagPayment::new(carol, 399)],
        _ => bag.payments().to_vec(),
    };

    let revealed = SecureTheBag::build(BagFormat::legacy(), revealed_asset, revealed_payments)?;

    let amount = match mismatch {
        BagMismatch::AmountTooLow => 999,
        BagMismatch::AmountTooHigh => 1001,
        _ => 1000,
    };

    let bags = match mismatch {
        BagMismatch::Unrevealed => vec![],
        _ => vec![revealed],
    };

    let tx = fund_bag(
        &mut sim,
        &mut ctx,
        &alice,
        id,
        bag.root_hash(),
        amount,
        bags,
    )?;

    // Without a matching reveal, the root is an ordinary send to an unknown puzzle hash
    let child = sent_child(&tx, bag.root_hash());
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, amount);
    assert_eq!(child.memos.p2_puzzle_hash, bag.root_hash());
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.transfer_type, TransferType::Sent);

    Ok(())
}

#[rstest]
fn test_clear_signing_secure_the_bag_single_payment(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset_kind: AssetKind,
    #[values(false, true)] clawback: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 1000)?;
    let bob: Bytes32 = "bob".tree_hash().into();

    let IssuedAsset {
        id,
        asset_id,
        hidden_puzzle_hash,
    } = issue_asset(&mut sim, &mut ctx, &alice, asset_kind, 1000)?;

    let payment = if clawback {
        BagPayment::from_clawback(ClawbackV2::new(
            alice.p2_puzzle_hash,
            bob,
            1000,
            1000,
            asset_id.is_some(),
        ))
    } else {
        BagPayment::new(bob, 1000)
    };

    let bag = SecureTheBag::build(
        BagFormat::legacy(),
        bag_asset(asset_id, hidden_puzzle_hash),
        [payment],
    )?;

    // A single payment isn't bagged unless it needs a node to create its clawback
    assert_eq!(bag.root_hash() == bob, !clawback);

    let tx = fund_bag(
        &mut sim,
        &mut ctx,
        &alice,
        id,
        bag.root_hash(),
        1000,
        vec![bag.clone()],
    )?;

    let child = sent_child(&tx, bag.root_hash());
    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 1000);

    if clawback {
        assert_eq!(child.memos.p2_puzzle_hash, bag.root_hash());
        assert_eq!(
            child.transfer_type,
            TransferType::SecureTheBag(SecureTheBagInfo {
                format: bag.format(),
                payments: vec![ParsedBagPayment {
                    payment,
                    transfer_type: TransferType::Sent,
                }],
            })
        );
    } else {
        assert_eq!(child.memos.p2_puzzle_hash, bob);
        assert_eq!(child.transfer_type, TransferType::Sent);
    }

    Ok(())
}

/// Measures how long it takes a signer to rebuild a revealed bag and verify that a vault transaction funds it.
///
/// Run in release mode with `BAG_PAYMENTS` (a comma separated list of sizes), `BAG_CLAWBACKS` (every nth payment
/// is a clawback, 0 for none) and optionally `BAG_LEAF_WIDTH`, for example:
/// `BAG_PAYMENTS=1000000 cargo test --release -p chia-sdk-driver bench_clear_signing_secure_the_bag -- --ignored --nocapture`
#[test]
#[ignore = "benchmark"]
fn bench_clear_signing_secure_the_bag() -> Result<()> {
    use std::time::Instant;

    let sizes: Vec<usize> = std::env::var("BAG_PAYMENTS")
        .unwrap_or_else(|_| "10000,100000,1000000".to_string())
        .split(',')
        .map(|size| size.trim().parse())
        .collect::<Result<_, _>>()?;
    let clawbacks: usize =
        std::env::var("BAG_CLAWBACKS").map_or(Ok(0), |clawbacks| clawbacks.parse())?;
    let leaf_width: Option<usize> = std::env::var("BAG_LEAF_WIDTH")
        .ok()
        .map(|width| width.parse())
        .transpose()?;
    let format = BagFormat::Legacy {
        leaf_width,
        sorted: true,
    };

    for size in sizes {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = TestVault::mint(&mut sim, &mut ctx, size as u64)?;

        let payments: Vec<BagPayment> = (0..size)
            .map(|i| {
                let puzzle_hash: Bytes32 = (i as u64).tree_hash().into();

                if clawbacks > 0 && i % clawbacks == 0 {
                    BagPayment::from_clawback(ClawbackV2::new(
                        alice.p2_puzzle_hash,
                        puzzle_hash,
                        1000,
                        1,
                        false,
                    ))
                } else {
                    BagPayment::new(puzzle_hash, 1)
                }
            })
            .collect();

        // The funding bag is dropped before measuring, so that only the revealed bag counts towards peak memory
        let root_hash = SecureTheBag::build(format, BagAsset::Xch, payments.clone())?.root_hash();
        let memos = ctx.hint(root_hash)?;

        let result = alice.spend(
            &mut sim,
            &mut ctx,
            &[Action::send(Id::Xch, root_hash, size as u64, memos)],
        )?;

        let start = Instant::now();
        let bag = SecureTheBag::build(format, BagAsset::Xch, payments)?;
        let build_time = start.elapsed();

        let start = Instant::now();
        let mut reveals = Reveals::from_coin_spends(&mut ctx, &result.spend_bundle.coin_spends)?;
        reveals.reveal_bag(bag);
        let tx = parse_vault_transaction(
            reveals,
            &mut ctx,
            alice.info.launcher_id,
            result.delegated_spend,
        )?;
        let parse_time = start.elapsed();

        let child = sent_child(&tx, root_hash);
        let TransferType::SecureTheBag(info) = &child.transfer_type else {
            panic!("expected a bag");
        };
        assert_eq!(info.payments.len(), size);

        println!(
            "{size} payments (clawbacks every {clawbacks}, width {}): rebuild {build_time:?}, parse {parse_time:?}, total {:?}",
            format.width(size),
            build_time + parse_time
        );
    }

    Ok(())
}

/// Spends a coin owned by the vault's default p2 puzzle hash with the given delegated spend,
/// and sends it a message from the vault (which includes the given vault conditions).
fn parse_linked_spend(
    ctx: &mut SpendContext,
    vault: &TestVault,
    coin: Coin,
    delegated_spend: Spend,
    vault_conditions: Conditions,
    extra_coin_spends: &[CoinSpend],
) -> Result<Result<VaultTransaction, DriverError>> {
    let delegated_puzzle_hash = ctx.tree_hash(delegated_spend.puzzle);

    let TestP2Puzzle::P2Singleton(p2_singleton) = vault.p2_puzzles[&vault.p2_puzzle_hash.into()]
    else {
        panic!("expected p2 singleton");
    };
    let spend = p2_singleton.spend(ctx, vault.info.custody_hash.into(), 1, delegated_spend)?;
    ctx.spend(coin, spend)?;

    let mut coin_spends = ctx.take();
    coin_spends.extend_from_slice(extra_coin_spends);

    let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
        | MessageFlags::COIN.encode(MessageSide::Receiver);
    let coin_id = ctx.alloc(&coin.coin_id())?;
    let vault_delegated_spend = ctx.delegated_spend(
        vault_conditions
            .send_message(mode, delegated_puzzle_hash.to_vec().into(), vec![coin_id])
            .create_coin(vault.info.custody_hash.into(), 1, Memos::None),
    )?;

    let reveals = Reveals::from_coin_spends(ctx, &coin_spends)?;

    Ok(parse_vault_transaction(
        reveals,
        ctx,
        vault.info.launcher_id,
        vault_delegated_spend,
    ))
}

fn vault_coin(vault: &TestVault, amount: u64) -> Coin {
    Coin::new(Bytes32::new([42; 32]), vault.p2_puzzle_hash, amount)
}
