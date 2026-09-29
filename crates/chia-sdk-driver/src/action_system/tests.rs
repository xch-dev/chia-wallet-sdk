use anyhow::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_test::{BlsPair, Simulator, sign_transaction};
use chia_sdk_types::{Condition, Conditions};
use indexmap::{IndexMap, indexmap};
use rstest::rstest;

use crate::{
    Action, AssetInfo, Cat, CatAssetInfo, Deltas, DriverError, HashedPtr, Id, NftAssetInfo, Offer,
    OfferAmounts, Outputs, Relation, RequestedPayments, RoyaltyInfo, SpendContext, SpendKind,
    Spends, TransferNftById, calculate_royalty_payments, calculate_trade_price_amounts,
    calculate_trade_prices,
};

fn keys(puzzle_hash: Bytes32, pk: PublicKey) -> IndexMap<Bytes32, PublicKey> {
    indexmap! { puzzle_hash => pk }
}

/// The conditions that each conditions spend would emit, without consuming the spends.
fn emitted_conditions(
    ctx: &mut SpendContext,
    spends: &Spends,
    deltas: &Deltas,
    relation: Relation,
) -> Result<Vec<(Bytes32, Conditions)>, DriverError> {
    let finished = spends.clone().prepare(ctx, deltas, relation)?;

    Ok(finished
        .unspent()
        .into_iter()
        .filter_map(|(asset, kind)| match kind {
            SpendKind::Conditions(spend) => Some((asset.coin().coin_id(), spend.finish())),
            SpendKind::Settlement(_) => None,
        })
        .collect())
}

fn count_conditions(
    conditions: &[(Bytes32, Conditions)],
    predicate: impl Fn(&Condition) -> bool,
) -> usize {
    conditions
        .iter()
        .map(|(_, conditions)| conditions.iter().filter(|c| predicate(c)).count())
        .sum()
}

fn build(
    ctx: &mut SpendContext,
    mut spends: Spends,
    actions: &[Action],
    relation: Relation,
    keys: &IndexMap<Bytes32, PublicKey>,
) -> Result<(Outputs, Vec<CoinSpend>), DriverError> {
    let deltas = spends.apply(ctx, actions)?;
    let outputs = spends.finish_with_keys(ctx, &deltas, relation, keys)?;
    Ok((outputs, ctx.take()))
}

fn balance(sim: &Simulator, puzzle_hash: Bytes32) -> u64 {
    sim.unspent_coins(puzzle_hash, false)
        .iter()
        .map(|coin| coin.amount)
        .sum()
}

fn cat_balance(sim: &Simulator, cat: &Cat, p2_puzzle_hash: Bytes32) -> u64 {
    balance(sim, cat.child(p2_puzzle_hash, 0).coin.puzzle_hash)
}

#[test]
fn test_insufficient_xch_is_rejected() {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let result = build(
        &mut ctx,
        spends,
        &[Action::send(Id::Xch, alice.puzzle_hash, 2, Memos::None)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    );

    assert!(matches!(result, Err(DriverError::InsufficientFunds)));
}

#[test]
fn test_insufficient_xch_for_fee_is_rejected() {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let result = build(
        &mut ctx,
        spends,
        &[
            Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None),
            Action::fee(1),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    );

    assert!(matches!(result, Err(DriverError::InsufficientFunds)));
}

#[test]
fn test_insufficient_cat_is_rejected() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let hint = ctx.hint(alice.puzzle_hash)?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let result = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(None, 1),
            Action::send(Id::New(0), alice.puzzle_hash, 2, hint),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    );

    assert!(matches!(result, Err(DriverError::InsufficientFunds)));

    Ok(())
}

#[test]
fn test_fee_only_transaction() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(10);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::fee(3)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(outputs.fee, 3);
    assert_eq!(outputs.reserved_fee, 3);
    assert_eq!(outputs.xch.len(), 1);
    assert_eq!(outputs.xch[0].amount, 7);
    assert_eq!(balance(&sim, alice.puzzle_hash), 7);

    Ok(())
}

#[test]
fn test_spend_multiple_xch_coins() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let bob = BlsPair::new(1);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);
    spends.add(sim.new_coin(alice.puzzle_hash, 2));
    spends.add(sim.new_coin(alice.puzzle_hash, 3));

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::send(Id::Xch, bob.puzzle_hash, 5, Memos::None),
            Action::fee(0),
        ],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    assert_eq!(coin_spends.len(), 3);

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(balance(&sim, bob.puzzle_hash), 5);
    assert_eq!(balance(&sim, alice.puzzle_hash), 1);

    Ok(())
}

#[test]
fn test_spend_multiple_cat_coins() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(6);
    let bob = BlsPair::new(1);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;
    let bob_hint = ctx.hint(bob.puzzle_hash)?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(None, 6),
            Action::send(Id::New(0), alice.puzzle_hash, 1, alice_hint),
            Action::send(Id::New(0), alice.puzzle_hash, 2, alice_hint),
            Action::send(Id::New(0), alice.puzzle_hash, 3, alice_hint),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let cats = outputs.cats[&Id::New(0)].clone();
    assert_eq!(cats.len(), 3);
    let asset_id = cats[0].info.asset_id;

    let mut spends = Spends::new(alice.puzzle_hash);
    for cat in &cats {
        spends.add(*cat);
    }

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::send(
            Id::Existing(asset_id),
            bob.puzzle_hash,
            5,
            bob_hint,
        )],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, &[alice.sk])?;

    let outputs = &outputs.cats[&Id::Existing(asset_id)];
    assert_eq!(outputs.len(), 2);
    assert_eq!(cat_balance(&sim, &cats[0], bob.puzzle_hash), 5);
    assert_eq!(cat_balance(&sim, &cats[0], alice.puzzle_hash), 1);

    Ok(())
}

#[test]
fn test_separate_change_puzzle_hash() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(10);
    let bob = BlsPair::new(1);

    let mut spends = Spends::with_separate_change_puzzle_hash(alice.puzzle_hash, bob.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::fee(1)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(outputs.xch.len(), 1);
    assert_eq!(outputs.xch[0].puzzle_hash, bob.puzzle_hash);
    assert_eq!(balance(&sim, bob.puzzle_hash), 9);

    Ok(())
}

#[test]
fn test_assert_concurrent_prevents_splitting() -> Result<()> {
    let mut sim = Simulator::new();

    let alice = sim.bls(5);
    let bob = BlsPair::new(1);
    let coins = [
        alice.coin,
        sim.new_coin(alice.puzzle_hash, 5),
        sim.new_coin(alice.puzzle_hash, 5),
    ];

    for (relation, splittable) in [(Relation::None, true), (Relation::AssertConcurrent, false)] {
        let mut ctx = SpendContext::new();

        let mut spends = Spends::new(alice.puzzle_hash);
        for coin in coins {
            spends.add(coin);
        }

        let (_, coin_spends) = build(
            &mut ctx,
            spends,
            &[Action::send(Id::Xch, bob.puzzle_hash, 3, Memos::None)],
            relation,
            &keys(alice.puzzle_hash, alice.pk),
        )?;

        // Drop the spend that creates the outputs, leaving the other coins to be spent as fees.
        let partial: Vec<CoinSpend> = coin_spends
            .iter()
            .filter(|cs| cs.coin.coin_id() != coins[0].coin_id())
            .cloned()
            .collect();

        assert_eq!(
            sim.clone()
                .spend_coins(partial, std::slice::from_ref(&alice.sk))
                .is_ok(),
            splittable
        );
    }

    Ok(())
}

#[test]
fn test_settlement_payments_cannot_be_stripped() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let first = sim.new_coin(SETTLEMENT_PAYMENT_HASH.into(), 5);
    let second = sim.new_coin(SETTLEMENT_PAYMENT_HASH.into(), 5);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(first);
    spends.add(second);
    spends.add(alice.coin);

    // Both settlement coins make an identical payment. If the notarized payments shared a nonce,
    // either spend could be removed from the bundle without failing the payment assertions, and
    // the removed settlement coin could then be claimed by anyone.
    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::send(Id::Xch, alice.puzzle_hash, 5, Memos::None),
            Action::send(Id::Xch, alice.puzzle_hash, 5, Memos::None),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    for stripped in [first, second] {
        let partial: Vec<CoinSpend> = coin_spends
            .iter()
            .filter(|cs| cs.coin.coin_id() != stripped.coin_id())
            .cloned()
            .collect();

        assert!(
            sim.clone()
                .spend_coins(partial, std::slice::from_ref(&alice.sk))
                .is_err()
        );
    }

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(balance(&sim, alice.puzzle_hash), 11);

    Ok(())
}

#[test]
fn test_settlement_only_spend_emits_conditions_once() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = BlsPair::new(0);
    let xch = sim.new_coin(SETTLEMENT_PAYMENT_HASH.into(), 10);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(xch);

    // With no coins that can emit conditions, the payment assertion for the change and the reserved
    // fee require exactly one intermediate coin to be created and spent.
    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::fee(1)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    assert_eq!(coin_spends.len(), 2);
    assert!(
        coin_spends
            .iter()
            .any(|cs| cs.coin.puzzle_hash == alice.puzzle_hash
                && cs.coin.parent_coin_info == xch.coin_id())
    );

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(balance(&sim, alice.puzzle_hash), 9);

    Ok(())
}

#[test]
fn test_settlement_only_cat_and_xch_spend() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(20);
    let hint = ctx.hint(alice.puzzle_hash)?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(None, 10),
            Action::send(Id::New(0), SETTLEMENT_PAYMENT_HASH.into(), 10, Memos::None),
            Action::send(Id::Xch, SETTLEMENT_PAYMENT_HASH.into(), 10, Memos::None),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let settlement_cat = outputs.cats[&Id::New(0)]
        .iter()
        .find(|cat| cat.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into())
        .copied()
        .expect("missing settlement cat");
    let settlement_xch = outputs
        .xch
        .iter()
        .find(|coin| coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into())
        .copied()
        .expect("missing settlement coin");

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(settlement_xch);
    spends.add(settlement_cat);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::send(Id::Xch, alice.puzzle_hash, 10, Memos::None),
            Action::send(
                Id::Existing(settlement_cat.info.asset_id),
                alice.puzzle_hash,
                10,
                hint,
            ),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    // Two settlement spends, and a single intermediate spend to assert both payments.
    assert_eq!(coin_spends.len(), 3);

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(balance(&sim, alice.puzzle_hash), 10);
    assert_eq!(cat_balance(&sim, &settlement_cat, alice.puzzle_hash), 10);

    Ok(())
}

#[test]
fn test_transactions_are_deterministic() -> Result<()> {
    let mut sim = Simulator::new();

    let alice = sim.bls(10);
    let settlement = sim.new_coin(SETTLEMENT_PAYMENT_HASH.into(), 10);

    let mut results = Vec::new();

    for _ in 0..2 {
        let mut ctx = SpendContext::new();

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(settlement);
        spends.add(alice.coin);

        // The second payment conflicts with the first, so an intermediate settlement coin is created from the settlement coin.
        let (_, coin_spends) = build(
            &mut ctx,
            spends,
            &[
                Action::mint_empty_nft(),
                Action::create_empty_did(),
                Action::settle(
                    Id::Xch,
                    NotarizedPayment::new(
                        Bytes32::new([1; 32]),
                        vec![Payment::new(alice.puzzle_hash, 3, Memos::None)],
                    ),
                ),
                Action::settle(
                    Id::Xch,
                    NotarizedPayment::new(
                        Bytes32::new([2; 32]),
                        vec![Payment::new(alice.puzzle_hash, 3, Memos::None)],
                    ),
                ),
                Action::fee(1),
            ],
            Relation::AssertConcurrent,
            &keys(alice.puzzle_hash, alice.pk),
        )?;

        assert!(coin_spends.iter().any(|cs| {
            cs.coin.parent_coin_info == settlement.coin_id()
                && cs.coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
        }));

        results.push(coin_spends);
    }

    assert_eq!(results[0], results[1]);

    sim.spend_coins(results.remove(0), &[alice.sk])?;

    Ok(())
}

#[rstest]
#[case::normal(None)]
#[case::revocable(Some(Bytes32::new([7; 32])))]
fn test_offer_cat_for_xch(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(100);
    let bob = sim.bls(51);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;

    // Alice issues a CAT
    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(hidden_puzzle_hash, 100),
            Action::send(Id::New(0), alice.puzzle_hash, 100, alice_hint),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let cat = outputs.cats[&Id::New(0)][0];
    let asset_id = cat.info.asset_id;

    // Alice offers 100 CAT for 50 XCH
    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch.push(NotarizedPayment::new(
        Offer::nonce(vec![cat.coin.coin_id()]),
        vec![Payment::new(alice.puzzle_hash, 50, alice_hint)],
    ));
    let asset_info = AssetInfo::new();

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(cat);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::send(
            Id::Existing(asset_id),
            SETTLEMENT_PAYMENT_HASH.into(),
            100,
            Memos::None,
        )],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    // Bob takes the offer and pays a fee
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(bob.coin);

    let mut actions = offer.requested_payments().actions();
    actions.push(Action::fee(1));

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    let bob_cats = &outputs.cats[&Id::Existing(asset_id)];
    assert_eq!(bob_cats.len(), 1);
    assert_eq!(bob_cats[0].info.p2_puzzle_hash, bob.puzzle_hash);
    assert_eq!(bob_cats[0].info.hidden_puzzle_hash, hidden_puzzle_hash);
    assert_eq!(cat_balance(&sim, &cat, bob.puzzle_hash), 100);
    assert_eq!(balance(&sim, alice.puzzle_hash), 50);
    assert_eq!(balance(&sim, bob.puzzle_hash), 0);

    Ok(())
}

#[rstest]
#[case::normal(None)]
#[case::revocable(Some(Bytes32::new([7; 32])))]
fn test_offer_xch_for_cat(#[case] hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(50);
    let bob = sim.bls(100);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;
    let bob_hint = ctx.hint(bob.puzzle_hash)?;

    // Bob issues a CAT
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(bob.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(hidden_puzzle_hash, 100),
            Action::send(Id::New(0), bob.puzzle_hash, 100, bob_hint),
        ],
        Relation::None,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&bob.sk))?;

    let cat = outputs.cats[&Id::New(0)][0];
    let asset_id = cat.info.asset_id;

    // Alice offers 50 XCH for 100 CAT
    let mut requested_payments = RequestedPayments::new();
    requested_payments.cats.insert(
        asset_id,
        vec![NotarizedPayment::new(
            Offer::nonce(vec![alice.coin.coin_id()]),
            vec![Payment::new(alice.puzzle_hash, 100, alice_hint)],
        )],
    );
    let mut asset_info = AssetInfo::new();
    asset_info.insert_cat(asset_id, CatAssetInfo::new(hidden_puzzle_hash))?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::send(
            Id::Xch,
            SETTLEMENT_PAYMENT_HASH.into(),
            50,
            Memos::None,
        )],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    // Bob takes the offer, paying the CAT from his own coin and receiving the XCH as change
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(cat);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &offer.requested_payments().actions(),
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    assert!(
        outputs
            .xch
            .iter()
            .any(|coin| coin.puzzle_hash == bob.puzzle_hash && coin.amount == 50)
    );
    assert_eq!(cat_balance(&sim, &cat, alice.puzzle_hash), 100);
    assert_eq!(cat_balance(&sim, &cat, bob.puzzle_hash), 0);
    assert_eq!(balance(&sim, bob.puzzle_hash), 50);

    Ok(())
}

#[test]
fn test_offer_nft_for_xch_with_royalty() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let bob = sim.bls(1030);
    let carol = BlsPair::new(2);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;

    // Alice mints an NFT with a 3% royalty to Carol
    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::mint_nft(
            HashedPtr::NIL,
            Bytes32::default(),
            carol.puzzle_hash,
            300,
            1,
        )],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let nft = outputs.nfts[&Id::New(0)];
    let nft_id = Id::Existing(nft.info.launcher_id);

    // Alice offers the NFT for 1000 XCH, and includes the trade price so the royalty is enforced
    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch.push(NotarizedPayment::new(
        Offer::nonce(vec![nft.coin.coin_id()]),
        vec![Payment::new(alice.puzzle_hash, 1000, alice_hint)],
    ));
    let asset_info = AssetInfo::new();

    let trade_prices = calculate_trade_prices(
        &calculate_trade_price_amounts(&requested_payments.amounts(), 1),
        &asset_info,
    );

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(nft);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::update_nft(
                nft_id,
                vec![],
                Some(TransferNftById::new(None, trade_prices)),
            ),
            Action::send(nft_id, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
        ],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    let royalties = offer.requested_royalty_amounts();
    assert_eq!(royalties.xch, 30);

    // Bob takes the offer and pays the royalty
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(bob.coin);

    let mut actions = offer.requested_payments().actions();
    actions.push(Action::settle_royalty(
        &mut ctx,
        Id::Xch,
        nft.info.launcher_id,
        carol.puzzle_hash,
        royalties.xch,
    )?);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    let bob_nft = outputs.nfts[&nft_id];
    assert_eq!(bob_nft.info.p2_puzzle_hash, bob.puzzle_hash);
    assert!(sim.coin_state(bob_nft.coin.coin_id()).is_some());
    assert_eq!(balance(&sim, alice.puzzle_hash), 1000);
    assert_eq!(balance(&sim, carol.puzzle_hash), 30);

    Ok(())
}

#[test]
fn test_offer_nft_without_royalty_payment_fails() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let bob = sim.bls(1000);
    let carol = BlsPair::new(2);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::mint_empty_royalty_nft(carol.puzzle_hash, 300)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let nft = outputs.nfts[&Id::New(0)];
    let nft_id = Id::Existing(nft.info.launcher_id);

    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch.push(NotarizedPayment::new(
        Offer::nonce(vec![nft.coin.coin_id()]),
        vec![Payment::new(alice.puzzle_hash, 1000, alice_hint)],
    ));
    let asset_info = AssetInfo::new();

    let trade_prices = calculate_trade_prices(
        &calculate_trade_price_amounts(&requested_payments.amounts(), 1),
        &asset_info,
    );

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(nft);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::update_nft(
                nft_id,
                vec![],
                Some(TransferNftById::new(None, trade_prices)),
            ),
            Action::send(nft_id, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
        ],
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    // Bob tries to take the offer without paying the royalty
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(bob.coin);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &offer.requested_payments().actions(),
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    assert!(
        sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))
            .is_err()
    );

    Ok(())
}

#[test]
fn test_change_identical_to_payment() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(2);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    // The change coin would be identical to the payment, so it must be created from an intermediate coin.
    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::send(Id::Xch, alice.puzzle_hash, 1, Memos::None)],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    assert_eq!(coin_spends.len(), 2);

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(outputs.xch.len(), 2);
    assert_ne!(outputs.xch[0], outputs.xch[1]);
    assert_eq!(
        outputs
            .xch
            .iter()
            .filter(|coin| sim
                .coin_state(coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none()))
            .count(),
        2
    );
    assert_eq!(balance(&sim, alice.puzzle_hash), 2);

    Ok(())
}

#[test]
fn test_relation_none_still_emits_settlement_assertions() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(0);
    let first = sim.new_coin(alice.puzzle_hash, 30);
    let second = sim.new_coin(alice.puzzle_hash, 30);
    let bob = sim.bls(0);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;

    // Alice offers 50 XCH for 10 of Bob's XCH, using two coins and no relation between them
    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch.push(NotarizedPayment::new(
        Offer::nonce(vec![first.coin_id(), second.coin_id()]),
        vec![Payment::new(alice.puzzle_hash, 10, alice_hint)],
    ));
    let asset_info = AssetInfo::new();

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(first);
    spends.add(second);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let actions = [Action::send(
        Id::Xch,
        SETTLEMENT_PAYMENT_HASH.into(),
        50,
        Memos::None,
    )];

    let deltas = spends.apply(&mut ctx, &actions)?;
    let emitted = emitted_conditions(&mut ctx, &spends, &deltas, Relation::None)?;

    // The requested payment assertion is emitted exactly once, and no relation is added.
    assert_eq!(
        count_conditions(&emitted, |c| matches!(
            c,
            Condition::AssertPuzzleAnnouncement(_)
        )),
        1
    );
    assert_eq!(
        count_conditions(&emitted, |c| matches!(
            c,
            Condition::AssertConcurrentSpend(_)
        )),
        0
    );

    spends.finish_with_keys(
        &mut ctx,
        &deltas,
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;
    let coin_spends = ctx.take();

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    // Bob takes the offer with a 0 amount coin, also with no relation
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(bob.coin);

    let actions = offer.requested_payments().actions();
    let deltas = spends.apply(&mut ctx, &actions)?;
    let emitted = emitted_conditions(&mut ctx, &spends, &deltas, Relation::None)?;

    // The settlement coin's payment to Alice and change to Bob are both asserted by Bob's coin.
    assert_eq!(emitted.len(), 1);
    assert_eq!(
        count_conditions(&emitted, |c| matches!(
            c,
            Condition::AssertPuzzleAnnouncement(_)
        )),
        2
    );
    assert_eq!(
        count_conditions(&emitted, |c| matches!(
            c,
            Condition::AssertConcurrentSpend(_)
        )),
        0
    );

    spends.finish_with_keys(
        &mut ctx,
        &deltas,
        Relation::None,
        &keys(bob.puzzle_hash, bob.pk),
    )?;
    let coin_spends = ctx.take();

    // Removing the settlement spend from the taker's side must fail, since the assertions remain.
    let partial: Vec<CoinSpend> = coin_spends
        .iter()
        .filter(|cs| cs.coin.puzzle_hash != SETTLEMENT_PAYMENT_HASH.into())
        .cloned()
        .collect();
    let signature = sign_transaction(&partial, std::slice::from_ref(&bob.sk))?;
    assert!(
        sim.clone()
            .new_transaction(SpendBundle::new(partial, signature))
            .is_err()
    );

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    assert_eq!(balance(&sim, alice.puzzle_hash), 20);
    assert_eq!(balance(&sim, bob.puzzle_hash), 40);

    Ok(())
}

#[test]
fn test_duplicate_xch_payments_use_intermediate_source() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(5);
    let bob = BlsPair::new(1);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::send(Id::Xch, bob.puzzle_hash, 1, Memos::None),
            Action::send(Id::Xch, bob.puzzle_hash, 1, Memos::None),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    // The second identical payment must come from an intermediate coin.
    assert_eq!(coin_spends.len(), 2);

    sim.spend_coins(coin_spends, &[alice.sk])?;

    let bob_coins: Vec<_> = outputs
        .xch
        .iter()
        .filter(|coin| coin.puzzle_hash == bob.puzzle_hash)
        .collect();
    assert_eq!(bob_coins.len(), 2);
    assert_ne!(bob_coins[0], bob_coins[1]);
    assert_eq!(sim.unspent_coins(bob.puzzle_hash, false).len(), 2);
    assert_eq!(balance(&sim, bob.puzzle_hash), 2);
    assert_eq!(balance(&sim, alice.puzzle_hash), 3);

    Ok(())
}

#[test]
fn test_duplicate_cat_payments_use_intermediate_source() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(5);
    let bob = BlsPair::new(1);
    let bob_hint = ctx.hint(bob.puzzle_hash)?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(None, 5),
            Action::send(Id::New(0), bob.puzzle_hash, 1, bob_hint),
            Action::send(Id::New(0), bob.puzzle_hash, 1, bob_hint),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, &[alice.sk])?;

    let cats = &outputs.cats[&Id::New(0)];
    let bob_cats: Vec<_> = cats
        .iter()
        .filter(|cat| cat.info.p2_puzzle_hash == bob.puzzle_hash)
        .collect();
    assert_eq!(bob_cats.len(), 2);
    assert_ne!(bob_cats[0].coin, bob_cats[1].coin);
    assert_eq!(cat_balance(&sim, &cats[0], bob.puzzle_hash), 2);
    assert_eq!(cat_balance(&sim, &cats[0], alice.puzzle_hash), 3);

    Ok(())
}

#[test]
fn test_duplicate_settlement_payments_use_intermediate_settlement_source() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(0);
    let bob = BlsPair::new(1);
    let settlement = sim.new_coin(SETTLEMENT_PAYMENT_HASH.into(), 10);

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(settlement);
    spends.add(alice.coin);

    let payment = Payment::new(bob.puzzle_hash, 3, Memos::None);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::settle(
                Id::Xch,
                NotarizedPayment::new(Bytes32::new([1; 32]), vec![payment.clone()]),
            ),
            Action::settle(
                Id::Xch,
                NotarizedPayment::new(Bytes32::new([2; 32]), vec![payment]),
            ),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    // The second payment is identical to the first, so it's made by an intermediate settlement
    // coin created from the original settlement coin.
    assert!(coin_spends.iter().any(|cs| {
        cs.coin.parent_coin_info == settlement.coin_id()
            && cs.coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
    }));

    sim.spend_coins(coin_spends, &[alice.sk])?;

    assert_eq!(
        outputs
            .xch
            .iter()
            .filter(|coin| coin.puzzle_hash == bob.puzzle_hash)
            .count(),
        2
    );
    assert_eq!(sim.unspent_coins(bob.puzzle_hash, false).len(), 2);
    assert_eq!(balance(&sim, bob.puzzle_hash), 6);
    assert_eq!(balance(&sim, alice.puzzle_hash), 4);

    Ok(())
}

#[test]
fn test_offer_royalty_nfts_and_cat_for_xch_and_cat() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(102);
    let bob = sim.bls(208);
    let bob_xch = sim.new_coin(bob.puzzle_hash, 1100);
    let carol = BlsPair::new(2);
    let dave = BlsPair::new(3);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;
    let bob_hint = ctx.hint(bob.puzzle_hash)?;

    // Alice mints two NFTs with different royalties, and issues a CAT
    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::mint_empty_royalty_nft(carol.puzzle_hash, 300),
            Action::mint_empty_royalty_nft(dave.puzzle_hash, 500),
            Action::single_issue_cat(None, 100),
            Action::send(Id::New(2), alice.puzzle_hash, 100, alice_hint),
        ],
        Relation::None,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&alice.sk))?;

    let first_nft = outputs.nfts[&Id::New(0)];
    let second_nft = outputs.nfts[&Id::New(1)];
    let alice_cat = outputs.cats[&Id::New(2)][0];

    // Bob issues a CAT
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(bob.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[
            Action::single_issue_cat(None, 208),
            Action::send(Id::New(0), bob.puzzle_hash, 208, bob_hint),
        ],
        Relation::None,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&bob.sk))?;

    let bob_cat = outputs.cats[&Id::New(0)][0];
    let bob_asset_id = bob_cat.info.asset_id;

    // Alice offers both NFTs and 100 of her CAT for 1000 XCH and 200 of Bob's CAT
    let nonce = Offer::nonce(vec![
        first_nft.coin.coin_id(),
        second_nft.coin.coin_id(),
        alice_cat.coin.coin_id(),
    ]);

    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch.push(NotarizedPayment::new(
        nonce,
        vec![Payment::new(alice.puzzle_hash, 1000, alice_hint)],
    ));
    requested_payments.cats.insert(
        bob_asset_id,
        vec![NotarizedPayment::new(
            nonce,
            vec![Payment::new(alice.puzzle_hash, 200, alice_hint)],
        )],
    );
    let mut asset_info = AssetInfo::new();
    asset_info.insert_cat(bob_asset_id, CatAssetInfo::new(None))?;

    let trade_prices = calculate_trade_prices(
        &calculate_trade_price_amounts(&requested_payments.amounts(), 2),
        &asset_info,
    );

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(first_nft);
    spends.add(second_nft);
    spends.add(alice_cat);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let mut actions = Vec::new();

    for nft in [first_nft, second_nft] {
        let id = Id::Existing(nft.info.launcher_id);
        actions.push(Action::update_nft(
            id,
            vec![],
            Some(TransferNftById::new(None, trade_prices.clone())),
        ));
        actions.push(Action::send(
            id,
            SETTLEMENT_PAYMENT_HASH.into(),
            1,
            Memos::None,
        ));
    }

    actions.push(Action::send(
        Id::Existing(alice_cat.info.asset_id),
        SETTLEMENT_PAYMENT_HASH.into(),
        100,
        Memos::None,
    ));

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    // Each NFT has a trade price of 500 XCH and 100 CAT.
    let royalties = offer.requested_royalty_amounts();
    assert_eq!(royalties.xch, 15 + 25);
    assert_eq!(royalties.cats[&bob_asset_id], 3 + 5);

    // Bob takes the offer and pays every royalty in both assets
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(bob_xch);
    spends.add(bob_cat);

    let mut actions = offer.requested_payments().actions();

    for (id, first_amount, second_amount) in [(Id::Xch, 15, 25), (Id::Existing(bob_asset_id), 3, 5)]
    {
        actions.push(Action::settle_royalty(
            &mut ctx,
            id,
            first_nft.info.launcher_id,
            carol.puzzle_hash,
            first_amount,
        )?);
        actions.push(Action::settle_royalty(
            &mut ctx,
            id,
            second_nft.info.launcher_id,
            dave.puzzle_hash,
            second_amount,
        )?);
    }

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    for nft in [first_nft, second_nft] {
        let bob_nft = outputs.nfts[&Id::Existing(nft.info.launcher_id)];
        assert_eq!(bob_nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert!(
            sim.coin_state(bob_nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );
    }

    assert_eq!(cat_balance(&sim, &alice_cat, bob.puzzle_hash), 100);
    assert_eq!(balance(&sim, alice.puzzle_hash), 1000);
    assert_eq!(cat_balance(&sim, &bob_cat, alice.puzzle_hash), 200);
    assert_eq!(balance(&sim, carol.puzzle_hash), 15);
    assert_eq!(balance(&sim, dave.puzzle_hash), 25);
    assert_eq!(cat_balance(&sim, &bob_cat, carol.puzzle_hash), 3);
    assert_eq!(cat_balance(&sim, &bob_cat, dave.puzzle_hash), 5);
    assert_eq!(balance(&sim, bob.puzzle_hash), 60);
    assert_eq!(cat_balance(&sim, &bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_offer_xch_for_royalty_nft() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1030);
    let bob = sim.bls(1);
    let carol = BlsPair::new(2);
    let alice_hint = ctx.hint(alice.puzzle_hash)?;

    // Bob mints an NFT with a 3% royalty to Carol
    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(bob.coin);

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &[Action::mint_empty_royalty_nft(carol.puzzle_hash, 300)],
        Relation::None,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    sim.spend_coins(coin_spends, std::slice::from_ref(&bob.sk))?;

    let nft = outputs.nfts[&Id::New(0)];
    let launcher_id = nft.info.launcher_id;

    // Alice offers 1000 XCH for the NFT, and pays the royalty up front
    let royalties = [RoyaltyInfo::new(launcher_id, carol.puzzle_hash, 300)];
    let offered_amounts = OfferAmounts {
        xch: 1000,
        cats: IndexMap::new(),
    };
    let royalty_payments = calculate_royalty_payments(
        &mut ctx,
        &calculate_trade_price_amounts(&offered_amounts, royalties.len()),
        &royalties,
    )?;

    let mut actions = vec![Action::send(
        Id::Xch,
        SETTLEMENT_PAYMENT_HASH.into(),
        1000,
        Memos::None,
    )];
    actions.extend(royalty_payments.actions());

    let mut requested_payments = RequestedPayments::new();
    requested_payments.nfts.insert(
        launcher_id,
        vec![NotarizedPayment::new(
            Offer::nonce(vec![alice.coin.coin_id()]),
            vec![Payment::new(alice.puzzle_hash, 1, alice_hint)],
        )],
    );
    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        launcher_id,
        NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), carol.puzzle_hash, 300),
    )?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice.coin);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut ctx, &asset_info)?);

    let (_, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(alice.puzzle_hash, alice.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[alice.sk])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?;

    assert_eq!(offer.offered_coins().amounts().xch, 1000);
    assert_eq!(offer.offered_royalty_amounts().xch, 30);

    // Bob takes the offer, revealing the trade price to the NFT so it can assert the royalty
    let trade_prices = calculate_trade_prices(
        &calculate_trade_price_amounts(&offer.offered_coins().amounts(), 1),
        offer.asset_info(),
    );

    let mut spends = Spends::new(bob.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(nft);

    let mut actions = vec![Action::update_nft(
        Id::Existing(launcher_id),
        vec![],
        Some(TransferNftById::new(None, trade_prices)),
    )];
    actions.extend(offer.requested_payments().actions());

    let (outputs, coin_spends) = build(
        &mut ctx,
        spends,
        &actions,
        Relation::AssertConcurrent,
        &keys(bob.puzzle_hash, bob.pk),
    )?;

    let signature = sign_transaction(&coin_spends, &[bob.sk])?;
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, signature)))?;

    let alice_nft = outputs.nfts[&Id::Existing(launcher_id)];
    assert_eq!(alice_nft.info.p2_puzzle_hash, alice.puzzle_hash);
    assert!(
        sim.coin_state(alice_nft.coin.coin_id())
            .is_some_and(|state| state.spent_height.is_none())
    );
    assert_eq!(balance(&sim, bob.puzzle_hash), 1000);
    assert_eq!(balance(&sim, carol.puzzle_hash), 30);
    assert_eq!(balance(&sim, alice.puzzle_hash), 0);

    Ok(())
}
