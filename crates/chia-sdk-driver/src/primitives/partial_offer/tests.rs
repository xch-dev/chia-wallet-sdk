use std::num::NonZeroU64;

use anyhow::Result;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    CoinProof, Memos,
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_test::{Simulator, sign_transaction};
use chia_sdk_types::{
    Conditions,
    puzzles::{PartialOfferPrice, PartialOfferSolution},
};
use clvm_traits::{FromClvm, ToClvm, clvm_list};
use clvm_utils::tree_hash;
use clvmr::Allocator;
use indexmap::indexmap;
use rstest::rstest;

use crate::{
    Action, Cat, DriverError, Id, Layer, PartialOffer, PartialOfferAsset, PartialOfferBundle,
    PartialOfferHint, PartialOfferInfo, Puzzle, Relation, SettlementLayer, SpendContext,
    SpendWithConditions, Spends, StandardLayer,
};

fn nz(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Xch,
    Cat,
    RevocableCat,
}

/// A wallet with some amount of a single asset, and optionally some XCH as well.
#[derive(Debug, Clone)]
struct Party {
    sk: SecretKey,
    pk: PublicKey,
    puzzle_hash: Bytes32,
    asset: PartialOfferAsset,
    id: Id,
    xch: Vec<Coin>,
    cat: Option<Cat>,
}

impl Party {
    fn new(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        kind: Kind,
        amount: u64,
        xch_amount: u64,
    ) -> Result<Self> {
        let pair = sim.bls(amount + xch_amount);

        if kind == Kind::Xch {
            return Ok(Self {
                sk: pair.sk,
                pk: pair.pk,
                puzzle_hash: pair.puzzle_hash,
                asset: PartialOfferAsset::Xch,
                id: Id::Xch,
                xch: vec![pair.coin],
                cat: None,
            });
        }

        let hidden_puzzle_hash = (kind == Kind::RevocableCat).then_some(Bytes32::new([7; 32]));
        let hint = ctx.hint(pair.puzzle_hash)?;

        let mut spends = Spends::new(pair.puzzle_hash);
        spends.add(pair.coin);

        let deltas = spends.apply(
            ctx,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, amount),
                Action::send(Id::New(0), pair.puzzle_hash, amount, hint),
            ],
        )?;
        let outputs = spends.finish_with_keys(
            ctx,
            &deltas,
            Relation::None,
            &indexmap! { pair.puzzle_hash => pair.pk },
        )?;
        sim.spend_coins(ctx.take(), std::slice::from_ref(&pair.sk))?;

        let cat = outputs.cats[&Id::New(0)][0];

        Ok(Self {
            sk: pair.sk,
            pk: pair.pk,
            puzzle_hash: pair.puzzle_hash,
            asset: PartialOfferAsset::Cat {
                asset_id: cat.info.asset_id,
                hidden_puzzle_hash,
            },
            id: Id::Existing(cat.info.asset_id),
            xch: outputs.xch,
            cat: Some(cat),
        })
    }

    fn spends(&self) -> Spends {
        let mut spends = Spends::new(self.puzzle_hash);
        for &coin in &self.xch {
            spends.add(coin);
        }
        if let Some(cat) = self.cat {
            spends.add(cat);
        }
        spends
    }

    fn build(
        &self,
        ctx: &mut SpendContext,
        mut spends: Spends,
        actions: &[Action],
        relation: Relation,
    ) -> Result<SpendBundle> {
        let deltas = spends.apply(ctx, actions)?;
        spends.finish_with_keys(
            ctx,
            &deltas,
            relation,
            &indexmap! { self.puzzle_hash => self.pk },
        )?;
        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, std::slice::from_ref(&self.sk))?;
        Ok(SpendBundle::new(coin_spends, signature))
    }

    /// Updates the party's coins to the unspent coins on-chain.
    fn refresh(self, sim: &Simulator, ctx: &mut SpendContext) -> Result<Self> {
        let mut cat = None;

        if let Some(old_cat) = self.cat {
            let puzzle_hash = old_cat.child(self.puzzle_hash, 0).coin.puzzle_hash;

            for coin in sim.unspent_coins(puzzle_hash, false) {
                let parent = sim
                    .coin_spend(coin.parent_coin_info)
                    .expect("parent is spent");
                let puzzle = ctx.alloc(&parent.puzzle_reveal)?;
                let solution = ctx.alloc(&parent.solution)?;
                let puzzle = Puzzle::parse(ctx, puzzle);

                assert!(cat.is_none(), "expected at most one CAT coin");
                cat = Cat::parse_children(ctx, parent.coin, puzzle, solution)?
                    .expect("parent is a CAT")
                    .into_iter()
                    .find(|cat| cat.coin == coin);
            }
        }

        Ok(Self {
            xch: sim.unspent_coins(self.puzzle_hash, false),
            cat,
            ..self
        })
    }

    /// The balance of a party's own asset, held by the given puzzle hash.
    fn balance_of(&self, sim: &Simulator, puzzle_hash: Bytes32) -> u64 {
        let puzzle_hash = self.asset.puzzle_hash(puzzle_hash);
        sim.unspent_coins(puzzle_hash, false)
            .iter()
            .map(|coin| coin.amount)
            .sum()
    }

    /// Locks `amount` of the party's asset in a partial offer, without broadcasting it.
    fn make(
        &self,
        ctx: &mut SpendContext,
        info: PartialOfferInfo,
        amount: u64,
    ) -> Result<PartialOfferBundle> {
        let memos = info.creation_memos(ctx)?;
        let spend_bundle = self.build(
            ctx,
            self.spends(),
            &[Action::send(self.id, info.p2_puzzle_hash(), amount, memos)],
            Relation::None,
        )?;
        Ok(PartialOfferBundle::from_maker_spend_bundle(
            ctx,
            spend_bundle,
            info,
        )?)
    }

    /// Fills a partial offer with the party's coins, and returns the spend bundle to submit.
    fn take(
        &self,
        ctx: &mut SpendContext,
        bundle: &PartialOfferBundle,
        requested_amount: u64,
        fee: u64,
    ) -> Result<(SpendBundle, Option<PartialOffer>)> {
        let fill = bundle.fill(ctx, requested_amount)?;

        let mut spends = self.spends();
        spends.add(fill.offer.offered_coins().clone());

        let mut actions = fill.actions();
        if fee > 0 {
            actions.push(Action::fee(fee));
        }

        let spend_bundle = self.build(ctx, spends, &actions, Relation::AssertConcurrent)?;

        Ok((fill.offer.take(spend_bundle), fill.child))
    }
}

fn xch_balance(sim: &Simulator, puzzle_hash: Bytes32) -> u64 {
    sim.unspent_coins(puzzle_hash, false)
        .iter()
        .map(|coin| coin.amount)
        .sum()
}

/// The coin spend of a coin, parsed as a partial offer fill.
fn parse_child(
    sim: &Simulator,
    ctx: &mut SpendContext,
    coin: Coin,
    requested_asset: Option<PartialOfferAsset>,
) -> Result<Option<PartialOffer>> {
    let coin_spend = sim.coin_spend(coin.coin_id()).expect("coin is spent");
    let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
    let solution = ctx.alloc(&coin_spend.solution)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    Ok(PartialOffer::parse_child(
        ctx,
        coin,
        puzzle,
        solution,
        requested_asset,
    )?)
}

#[rstest]
#[case::xch_for_cat(Kind::Xch, Kind::Cat)]
#[case::cat_for_xch(Kind::Cat, Kind::Xch)]
#[case::cat_for_cat(Kind::Cat, Kind::Cat)]
#[case::xch_for_revocable_cat(Kind::Xch, Kind::RevocableCat)]
#[case::revocable_cat_for_xch(Kind::RevocableCat, Kind::Xch)]
#[case::revocable_cat_for_cat(Kind::RevocableCat, Kind::Cat)]
#[case::cat_for_revocable_cat(Kind::Cat, Kind::RevocableCat)]
#[case::revocable_cat_for_revocable_cat(Kind::RevocableCat, Kind::RevocableCat)]
fn test_partial_offer(
    #[case] offered: Kind,
    #[case] requested: Kind,
    #[values(false, true)] with_terms: bool,
) -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, offered, 1000, 0)?;
    let bob = Party::new(&mut sim, ctx, requested, 400, 100)?;

    // Alice offers 1000 of her asset for 400 of Bob's, which is 5 for every 2
    let mut info = PartialOfferInfo::from_amounts(
        alice.asset,
        bob.asset,
        alice.puzzle_hash,
        nz(1000),
        nz(400),
    );
    assert_eq!(info.price, PartialOfferPrice::new(nz(5), nz(2)));

    let required_fee = if with_terms { 10 } else { 0 };

    if with_terms {
        info = info
            .with_min_requested_amount(nz(50))
            .with_expiration(sim.next_timestamp() + 1000)
            .with_required_fee(required_fee);
    }

    let bundle = alice.make(ctx, info, 1000)?;
    let eve_coin = bundle.partial_offer().coin;
    assert_eq!(eve_coin.amount, 1000);

    // The offer is shared and decoded, and is the same
    let spend_bundle = bundle.to_spend_bundle(ctx)?;
    let bundle = PartialOfferBundle::from_spend_bundle(ctx, &spend_bundle)?;
    assert_eq!(bundle.partial_offer().coin, eve_coin);
    assert_eq!(bundle.partial_offer().info, info);

    // Bob pays 100, which releases 250, and also creates the offer coin
    let (spend_bundle, child) = bob.take(ctx, &bundle, 100, 1)?;
    sim.new_transaction(spend_bundle)?;

    let child = child.expect("offer isn't fully filled");
    assert_eq!(child.coin.amount, 750);
    assert!(sim.coin_state(child.coin.coin_id()).is_some());
    assert_eq!(bob.balance_of(&sim, alice.puzzle_hash), 100);

    // Bob's XCH pays for the fees (and for the offer, if XCH is requested), and his payout is
    // added to it if XCH is offered
    let mut bob_xch = if requested == Kind::Xch { 500 } else { 100 };
    bob_xch -= required_fee + 1;
    if requested == Kind::Xch {
        bob_xch -= 100;
    }
    if offered == Kind::Xch {
        bob_xch += 250;
    } else {
        assert_eq!(alice.balance_of(&sim, bob.puzzle_hash), 250);
    }
    assert_eq!(xch_balance(&sim, bob.puzzle_hash), bob_xch);

    // The first offer coin is hinted to Alice, and the child can be parsed from its spend
    assert!(
        sim.hinted_coins(alice.puzzle_hash)
            .contains(&eve_coin.coin_id())
    );
    assert_eq!(
        parse_child(&sim, ctx, eve_coin, Some(bob.asset))?,
        Some(child)
    );

    if requested != Kind::Xch {
        assert!(matches!(
            parse_child(&sim, ctx, eve_coin, None),
            Err(e) if matches!(e.downcast_ref::<DriverError>(), Some(DriverError::MissingAssetInfo))
        ));
    }

    // Bob takes the rest, which costs 300
    let requested_amount = child.max_requested_amount().expect("offer is fillable");
    assert_eq!(requested_amount, 300);

    let bob = bob.refresh(&sim, ctx)?;

    let (spend_bundle, grandchild) = bob.take(
        ctx,
        &PartialOfferBundle::from_coin(child),
        requested_amount,
        0,
    )?;
    sim.new_transaction(spend_bundle)?;

    assert_eq!(grandchild, None);
    assert_eq!(parse_child(&sim, ctx, child.coin, Some(bob.asset))?, None);
    assert_eq!(alice.balance_of(&sim, info.p2_puzzle_hash()), 0);
    assert_eq!(bob.balance_of(&sim, alice.puzzle_hash), 400);

    bob_xch -= required_fee;
    if requested == Kind::Xch {
        bob_xch -= 300;
    }
    if offered == Kind::Xch {
        bob_xch += 750;
    } else {
        assert_eq!(alice.balance_of(&sim, bob.puzzle_hash), 1000);
    }
    assert_eq!(xch_balance(&sim, bob.puzzle_hash), bob_xch);

    Ok(())
}

#[rstest]
#[case::xch(Kind::Xch)]
#[case::cat(Kind::Cat)]
#[case::revocable_cat(Kind::RevocableCat)]
fn test_partial_offer_claw_back(#[case] offered: Kind) -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, offered, 1000, 0)?;
    let bob = Party::new(&mut sim, ctx, Kind::Cat, 400, 0)?;

    let info = PartialOfferInfo::from_amounts(
        alice.asset,
        bob.asset,
        alice.puzzle_hash,
        nz(1000),
        nz(400),
    )
    .with_expiration(sim.next_timestamp() + 1000);

    // Alice creates the offer coin on-chain right away
    let bundle = alice.make(ctx, info, 1000)?;
    sim.new_transaction(bundle.spend_bundle().clone())?;

    let bundle = PartialOfferBundle::from_coin(*bundle.partial_offer());
    let (spend_bundle, child) = bob.take(ctx, &bundle, 200, 0)?;
    sim.new_transaction(spend_bundle)?;

    let child = child.expect("offer isn't fully filled");
    assert_eq!(child.coin.amount, 500);

    // Alice claws back the rest after it expires
    sim.pass_time(2000);

    let hint = ctx.hint(alice.puzzle_hash)?;
    let inner_spend = StandardLayer::new(alice.pk).spend_with_conditions(
        ctx,
        Conditions::new().create_coin(alice.puzzle_hash, 500, hint),
    )?;
    child.claw_back(ctx, inner_spend)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

    assert_eq!(alice.balance_of(&sim, alice.puzzle_hash), 500);
    assert_eq!(alice.balance_of(&sim, bob.puzzle_hash), 500);
    assert_eq!(parse_child(&sim, ctx, child.coin, Some(bob.asset))?, None);

    Ok(())
}

#[test]
fn test_partial_offer_expired() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::Xch, 1000, 0)?;
    let bob = Party::new(&mut sim, ctx, Kind::Cat, 400, 0)?;

    let info = PartialOfferInfo::from_amounts(
        alice.asset,
        bob.asset,
        alice.puzzle_hash,
        nz(1000),
        nz(400),
    )
    .with_expiration(sim.next_timestamp() + 1000);

    let bundle = alice.make(ctx, info, 1000)?;
    sim.pass_time(2000);

    let (spend_bundle, _) = bob.take(ctx, &bundle, 100, 0)?;
    let error = sim.new_transaction(spend_bundle).unwrap_err();
    assert!(format!("{error:?}").contains("AssertBeforeSecondsAbsoluteFailed"));

    Ok(())
}

#[test]
fn test_partial_offer_required_fee_must_be_paid() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::Cat, 1000, 0)?;
    let bob = Party::new(&mut sim, ctx, Kind::Xch, 400, 100)?;

    let info = PartialOfferInfo::from_amounts(
        alice.asset,
        bob.asset,
        alice.puzzle_hash,
        nz(1000),
        nz(400),
    )
    .with_required_fee(50);

    let bundle = alice.make(ctx, info, 1000)?;
    let fill = bundle.fill(ctx, 100)?;

    // Bob leaves out the required fee
    let mut spends = bob.spends();
    spends.add(fill.offer.offered_coins().clone());
    let spend_bundle = bob.build(
        ctx,
        spends,
        &fill.offer.requested_payments().actions(),
        Relation::AssertConcurrent,
    )?;

    let error = sim
        .new_transaction(fill.offer.take(spend_bundle))
        .unwrap_err();
    assert!(format!("{error:?}").contains("ReserveFeeConditionFailed"));

    Ok(())
}

#[test]
fn test_partial_offer_fill_amounts() {
    let info = PartialOfferInfo::from_amounts(
        PartialOfferAsset::Xch,
        PartialOfferAsset::Cat {
            asset_id: Bytes32::new([1; 32]),
            hidden_puzzle_hash: None,
        },
        Bytes32::new([2; 32]),
        nz(1),
        nz(3),
    )
    .with_min_requested_amount(nz(2));

    let offer = PartialOffer::new(
        Coin::new(Bytes32::new([3; 32]), info.puzzle_hash(), 10),
        None,
        info,
    );

    // Below the minimum
    assert!(matches!(
        offer.offered_amount(1),
        Err(DriverError::InvalidPartialOfferFill)
    ));

    // Releases nothing, since 2 / 3 rounds down to 0
    assert!(matches!(
        offer.offered_amount(2),
        Err(DriverError::InvalidPartialOfferFill)
    ));

    assert_eq!(offer.offered_amount(3).ok(), Some(1));
    assert_eq!(offer.offered_amount(5).ok(), Some(1));
    assert_eq!(offer.offered_amount(30).ok(), Some(10));

    // Releases more than the coin has left
    assert!(matches!(
        offer.offered_amount(33),
        Err(DriverError::InvalidPartialOfferFill)
    ));

    assert_eq!(offer.max_requested_amount(), Some(30));
    assert_eq!(offer.child(30), None);
    assert_eq!(offer.child(3).map(|child| child.coin.amount), Some(9));

    // Large prices don't overflow
    let info = PartialOfferInfo {
        price: PartialOfferPrice::new(nz(u64::MAX), nz(u64::MAX - 1)),
        ..info
    };
    assert_eq!(info.offered_amount(u64::MAX - 1), Some(u64::MAX));
    assert_eq!(info.offered_amount(u64::MAX), None);
    assert_eq!(info.requested_amount(u64::MAX), Some(u64::MAX - 1));
}

#[test]
fn test_partial_offer_asset_clvm() -> Result<()> {
    let mut allocator = Allocator::new();
    let asset_id = Bytes32::new([1; 32]);
    let hidden_puzzle_hash = Bytes32::new([2; 32]);

    for asset in [
        PartialOfferAsset::Xch,
        PartialOfferAsset::Cat {
            asset_id,
            hidden_puzzle_hash: None,
        },
        PartialOfferAsset::Cat {
            asset_id,
            hidden_puzzle_hash: Some(hidden_puzzle_hash),
        },
    ] {
        let ptr = asset.to_clvm(&mut allocator)?;
        assert_eq!(PartialOfferAsset::from_clvm(&allocator, ptr)?, asset);
    }

    let xch = PartialOfferAsset::Xch.to_clvm(&mut allocator)?;
    let expected = clvm_list!((), ()).to_clvm(&mut allocator)?;
    assert_eq!(tree_hash(&allocator, xch), tree_hash(&allocator, expected));

    let invalid = clvm_list!((), hidden_puzzle_hash).to_clvm(&mut allocator)?;
    assert!(PartialOfferAsset::from_clvm(&allocator, invalid).is_err());

    Ok(())
}

#[test]
fn test_partial_offer_unsupported_conditions() -> Result<()> {
    let ctx = &mut SpendContext::new();

    let info = PartialOfferInfo::from_amounts(
        PartialOfferAsset::Xch,
        PartialOfferAsset::Cat {
            asset_id: Bytes32::new([1; 32]),
            hidden_puzzle_hash: None,
        },
        Bytes32::new([2; 32]),
        nz(1000),
        nz(400),
    )
    .with_required_fee(10);

    let hint = info.alloc_hint(ctx, None)?;
    assert_eq!(PartialOfferInfo::parse_hint(ctx, hint)?, (info, None));

    // The persistent conditions can't contain anything that isn't displayed
    for conditions in [
        Conditions::new()
            .reserve_fee(10)
            .create_coin(Bytes32::default(), 1, Memos::None),
        Conditions::new().reserve_fee(10).reserve_fee(10),
        Conditions::new()
            .assert_before_seconds_absolute(5)
            .reserve_fee(10)
            .assert_before_seconds_absolute(1),
    ] {
        let hint = ctx.alloc(&PartialOfferHint {
            lineage_proof: None,
            offered_asset: info.offered_asset,
            requested_asset: info.requested_asset,
            price: info.price,
            maker_puzzle_hash: info.maker_puzzle_hash,
            inner_puzzle: clvm_traits::clvm_quote!(conditions),
            min_other_asset_amount_minus_one: 0,
        })?;

        assert!(matches!(
            PartialOfferInfo::parse_hint(ctx, hint),
            Err(DriverError::UnsupportedPartialOfferConditions)
        ));
    }

    Ok(())
}

#[test]
fn test_partial_offer_siblings_rejected() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::Xch, 1000, 0)?;
    let cat = PartialOfferAsset::Cat {
        asset_id: Bytes32::new([1; 32]),
        hidden_puzzle_hash: None,
    };

    let info =
        PartialOfferInfo::from_amounts(alice.asset, cat, alice.puzzle_hash, nz(1000), nz(400));
    let other_info =
        PartialOfferInfo::from_amounts(alice.asset, cat, alice.puzzle_hash, nz(1000), nz(500));

    // Two coins with the same terms
    let memos = info.creation_memos(ctx)?;
    let spend_bundle = alice.build(
        ctx,
        alice.spends(),
        &[
            Action::send(Id::Xch, info.p2_puzzle_hash(), 300, memos),
            Action::send(Id::Xch, info.p2_puzzle_hash(), 700, memos),
        ],
        Relation::None,
    )?;
    assert!(matches!(
        PartialOfferBundle::from_maker_spend_bundle(ctx, spend_bundle, info),
        Err(DriverError::PartialOfferSiblings)
    ));

    // Two coins with different prices, which would still share a payment nonce
    let other_memos = other_info.creation_memos(ctx)?;
    let spend_bundle = alice.build(
        ctx,
        alice.spends(),
        &[
            Action::send(Id::Xch, info.p2_puzzle_hash(), 300, memos),
            Action::send(Id::Xch, other_info.p2_puzzle_hash(), 700, other_memos),
        ],
        Relation::None,
    )?;
    assert!(matches!(
        PartialOfferBundle::from_maker_spend_bundle(ctx, spend_bundle, info),
        Err(DriverError::PartialOfferSiblings)
    ));

    // Without the creation memos
    let spend_bundle = alice.build(
        ctx,
        alice.spends(),
        &[Action::send(
            Id::Xch,
            info.p2_puzzle_hash(),
            1000,
            Memos::None,
        )],
        Relation::None,
    )?;
    assert!(matches!(
        PartialOfferBundle::from_maker_spend_bundle(ctx, spend_bundle, info),
        Err(DriverError::InvalidMemo)
    ));

    Ok(())
}

/// The reference implementation created the offer coin from an unsigned settlement payments spend,
/// which anyone could change to take the funds.
#[test]
fn test_partial_offer_unsigned_creation_rejected() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::Xch, 1000, 0)?;
    let cat = PartialOfferAsset::Cat {
        asset_id: Bytes32::new([1; 32]),
        hidden_puzzle_hash: None,
    };
    let info =
        PartialOfferInfo::from_amounts(alice.asset, cat, alice.puzzle_hash, nz(1000), nz(400));

    let mut spend_bundle = alice.build(
        ctx,
        alice.spends(),
        &[Action::send(
            Id::Xch,
            SETTLEMENT_PAYMENT_HASH.into(),
            1000,
            Memos::None,
        )],
        Relation::None,
    )?;

    let settlement_coin = Coin::new(alice.xch[0].coin_id(), SETTLEMENT_PAYMENT_HASH.into(), 1000);
    let memos = info.creation_memos(ctx)?;
    let spend = SettlementLayer.construct_spend(
        ctx,
        SettlementPaymentsSolution::new(vec![NotarizedPayment::new(
            Bytes32::default(),
            vec![Payment::new(info.p2_puzzle_hash(), 1000, memos)],
        )]),
    )?;
    spend_bundle.coin_spends.push(CoinSpend::new(
        settlement_coin,
        ctx.serialize(&spend.puzzle)?,
        ctx.serialize(&spend.solution)?,
    ));

    assert!(matches!(
        PartialOfferBundle::from_maker_spend_bundle(ctx, spend_bundle, info),
        Err(DriverError::UnsignedPartialOfferCreation)
    ));

    Ok(())
}

/// The payout is created by the unsigned partial offer solution, so it must be asserted by the
/// taker. Otherwise, it could be redirected after the taker signs.
#[test]
fn test_partial_offer_payout_cannot_be_redirected() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::Xch, 1000, 0)?;
    let bob = Party::new(&mut sim, ctx, Kind::Cat, 400, 0)?;
    let eve = Bytes32::new([9; 32]);

    let info = PartialOfferInfo::from_amounts(
        alice.asset,
        bob.asset,
        alice.puzzle_hash,
        nz(1000),
        nz(400),
    );
    let bundle = alice.make(ctx, info, 1000)?;
    sim.new_transaction(bundle.spend_bundle().clone())?;

    let partial_offer = *bundle.partial_offer();
    let (spend_bundle, _) = bob.take(ctx, &PartialOfferBundle::from_coin(partial_offer), 100, 0)?;
    let mut redirected = spend_bundle.clone();

    // Eve sends the payout to herself instead
    let partial_puzzle = info.construct_partial_puzzle(ctx)?;
    let partial_solution = ctx.alloc(&PartialOfferSolution {
        my_proof: CoinProof {
            parent_coin_info: partial_offer.coin.parent_coin_info,
            inner_puzzle_hash: info.p2_puzzle_hash(),
            amount: partial_offer.coin.amount,
        },
        other_asset_amount: 100,
        create_coin_rest: Some(clvm_list!(eve, 250)),
        cat_maker_solution: (),
        inner_puzzle_solution: (),
    })?;
    let spend = partial_offer.spend_leaf(
        ctx,
        crate::Spend::new(partial_puzzle, partial_solution),
        info.partial_puzzle_hash().into(),
    )?;

    let coin_spend = redirected
        .coin_spends
        .iter_mut()
        .find(|coin_spend| coin_spend.coin == partial_offer.coin)
        .expect("partial offer coin is spent");
    coin_spend.puzzle_reveal = ctx.serialize(&spend.puzzle)?;
    coin_spend.solution = ctx.serialize(&spend.solution)?;

    assert!(sim.new_transaction(redirected).is_err());
    assert!(sim.unspent_coins(eve, false).is_empty());

    // The original is still valid
    sim.new_transaction(spend_bundle)?;
    assert_eq!(xch_balance(&sim, bob.puzzle_hash), 250);

    Ok(())
}

#[cfg(feature = "offer-compression")]
#[test]
fn test_encode_partial_offer() -> Result<()> {
    use crate::{decode_partial_offer, encode_partial_offer};

    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let alice = Party::new(&mut sim, ctx, Kind::RevocableCat, 1000, 0)?;
    let cat = PartialOfferAsset::Cat {
        asset_id: Bytes32::new([1; 32]),
        hidden_puzzle_hash: None,
    };
    let info =
        PartialOfferInfo::from_amounts(alice.asset, cat, alice.puzzle_hash, nz(1000), nz(400))
            .with_required_fee(10);

    let bundle = alice.make(ctx, info, 1000)?;
    let partial_offer = *bundle.partial_offer();

    let spend_bundle = bundle.to_spend_bundle(ctx)?;
    let text = encode_partial_offer(&spend_bundle)?;
    assert!(text.starts_with("partial1"));
    assert_eq!(decode_partial_offer(&text)?, spend_bundle);

    let decoded = PartialOfferBundle::from_spend_bundle(ctx, &decode_partial_offer(&text)?)?;
    assert_eq!(*decoded.partial_offer(), partial_offer);

    Ok(())
}
