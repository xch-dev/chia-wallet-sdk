use anyhow::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_test::Simulator;
use chia_sdk_types::{Condition, Conditions};
use clvm_utils::{ToTreeHash, tree_hash};
use rstest::rstest;

use crate::{
    Action, CustodyInfo, Deltas, Id, ParsedAsset, SpendContext, Spends, TestVault, VaultOutput,
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
        let result = alice.spend(
            sim,
            ctx,
            &[Action::single_issue_cat(hidden_puzzle_hash, amount)],
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

#[rstest]
fn test_clear_signing_vault_child() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 0)?;

    let result = alice.spend(&mut sim, &mut ctx, &[])?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(
        tx.vault_child,
        Some(VaultOutput::new(alice.custody_hash().into(), 1))
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
        &[Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None)],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    // Because we spent another coin, we know the launcher id and p2 puzzle hashes.
    assert_eq!(tx.launcher_id, Some(alice.info.launcher_id));
    assert_eq!(tx.p2_puzzle_hashes, vec![alice.puzzle_hash]);
    assert_eq!(tx.spends.len(), 1);

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
        &[Action::send(id, alice.puzzle_hash, 42, memos)],
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
    assert_eq!(p2_singleton.p2_puzzle_hash, alice.puzzle_hash);
    assert_eq!(spend.children.len(), 1);

    let child = &spend.children[0];

    check_asset(&child.asset, asset_id, hidden_puzzle_hash, 42);
    assert_eq!(child.asset.coin().amount, 42);
    assert_eq!(child.memos.clawback, None);
    assert_eq!(child.memos.p2_puzzle_hash, alice.puzzle_hash);
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
            Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
            Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
        ],
    )?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 2);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child_1 = &spend.children[0];
    assert_eq!(child_1.asset.coin().amount, 1);
    assert_eq!(child_1.memos.p2_puzzle_hash, alice.puzzle_hash);

    let intermediate_child = &spend.children[1];
    assert_eq!(intermediate_child.asset.coin().amount, 0);
    assert_eq!(intermediate_child.memos.p2_puzzle_hash, alice.puzzle_hash);

    let intermediate_spend = &tx.spends[1];
    assert_eq!(intermediate_spend.children.len(), 1);
    assert_eq!(
        intermediate_spend.asset.coin().coin_id(),
        intermediate_child.asset.coin().coin_id()
    );

    let child_2 = &intermediate_spend.children[0];
    assert_eq!(child_2.asset.coin().amount, 1);
    assert_eq!(child_2.memos.p2_puzzle_hash, alice.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_clear_signing_intermediate_delegated_conditions() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = TestVault::mint(&mut sim, &mut ctx, 2)?;

    let delegated_spend =
        ctx.delegated_spend(Conditions::new().create_coin(alice.puzzle_hash, 1, Memos::None))?;
    let puzzle_hash = tree_hash(&ctx, delegated_spend.puzzle).into();

    let actions = [
        Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
        Action::send(Id::Xch, puzzle_hash, 1, Memos::None),
    ];

    let mut spends = Spends::new(alice.puzzle_hash);
    alice.select_coins(&sim, &mut spends, &Deltas::from_actions(&actions))?;

    let input_coin_id = spends.xch.items[0].asset.coin_id();
    let intermediate_coin = Coin::new(input_coin_id, puzzle_hash, 1);

    ctx.spend(intermediate_coin, delegated_spend)?;
    spends
        .conditions
        .required
        .push(Condition::assert_concurrent_spend(
            intermediate_coin.coin_id(),
        ));

    let result = alice.custom_spend(&mut sim, &mut ctx, &actions, spends, Conditions::new())?;

    let tx = parse_vault_transaction(&mut ctx, result.delegated_spend, result.coin_spends, vec![])?;

    assert_eq!(tx.spends.len(), 2);

    let spend = &tx.spends[0];
    assert_eq!(spend.children.len(), 2);

    let child_1 = &spend.children[0];
    assert_eq!(child_1.asset.coin().amount, 1);
    assert_eq!(child_1.memos.p2_puzzle_hash, alice.puzzle_hash);

    let intermediate_child = &spend.children[1];
    assert_eq!(intermediate_child.asset.coin().amount, 1);
    assert_eq!(intermediate_child.memos.p2_puzzle_hash, puzzle_hash);

    let intermediate_spend = &tx.spends[1];
    assert_eq!(intermediate_spend.children.len(), 1);
    assert_eq!(
        intermediate_spend.asset.coin().coin_id(),
        intermediate_child.asset.coin().coin_id()
    );

    let child_2 = &intermediate_spend.children[0];
    assert_eq!(child_2.asset.coin().amount, 1);
    assert_eq!(child_2.memos.p2_puzzle_hash, alice.puzzle_hash);

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
