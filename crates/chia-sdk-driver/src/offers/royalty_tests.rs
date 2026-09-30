//! Royalty behavior of offers, validated against the simulator.
//!
//! An NFT spent into settlement reveals a list of trade prices in its transfer condition. For each
//! trade price, the royalty transfer program asserts a puzzle announcement from the trade price's
//! settlement puzzle hash for a notarized payment of exactly
//! `(launcher_id ((royalty_puzzle_hash floor(amount * basis_points / 10000) (royalty_puzzle_hash))))`.
//! The settlement puzzle rejects payments that aren't positive, so a trade price whose royalty
//! rounds to zero can never be paid.
//!
//! Those trade prices are chosen by each maker for their own NFTs, so royalties owed when taking an
//! aggregated offer can't be derived from its combined totals. Buyers requesting an NFT pay its
//! royalty up front, and the taker's NFT has to reveal trade prices matching those payments.

use std::slice;

use chia_bls::{PublicKey, Signature};
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    Memos,
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    offer::{NotarizedPayment, Payment},
    singleton::SingletonSolution,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_test::{BlsPair, Simulator, SimulatorError, sign_transaction};
use chia_sdk_types::{conditions::TradePrice, puzzles::SettlementPayment};
use clvm_traits::clvm_list;
use clvmr::NodePtr;
use indexmap::{IndexMap, indexmap};

use crate::{
    Action, AssetInfo, Cat, CatAssetInfo, CatInfo, CatSpend, DriverError, HashedPtr, Id, Layer,
    Nft, NftAssetInfo, Offer, OfferAmounts, OfferCoins, Outputs, Relation, RequestedPayments,
    Spend, SpendContext, Spends, TransferNftById, calculate_nft_royalty,
    calculate_trade_price_amounts, calculate_trade_prices, coin_amount, payable_trade_prices,
};

const ROYALTY_A: Bytes32 = Bytes32::new([1; 32]);
const ROYALTY_B: Bytes32 = Bytes32::new([2; 32]);
const ROYALTY_C: Bytes32 = Bytes32::new([3; 32]);
const HIDDEN_PUZZLE_HASH: Bytes32 = Bytes32::new([7; 32]);

fn keys(pair: &BlsPair) -> IndexMap<Bytes32, PublicKey> {
    indexmap! { pair.puzzle_hash => pair.pk }
}

fn xch(amount: u64) -> OfferAmounts {
    OfferAmounts {
        xch: amount.into(),
        cats: IndexMap::new(),
    }
}

fn cat(asset_id: Bytes32, amount: u64) -> OfferAmounts {
    OfferAmounts {
        xch: 0,
        cats: indexmap! { asset_id => amount.into() },
    }
}

fn xch_trade_price(amount: u64) -> TradePrice {
    TradePrice::new(amount, SETTLEMENT_PAYMENT_HASH.into())
}

fn cat_trade_price(asset_id: Bytes32, amount: u64) -> TradePrice {
    TradePrice::new(
        amount,
        CatInfo::new(asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
            .puzzle_hash()
            .into(),
    )
}

fn is_validation_error(error: &anyhow::Error, code: ErrorCode) -> bool {
    matches!(
        error.downcast_ref::<SimulatorError>(),
        Some(SimulatorError::Validation(actual)) if *actual == code
    )
}

fn revocable_cat_trade_price(asset_id: Bytes32, amount: u64) -> TradePrice {
    TradePrice::new(
        amount,
        CatInfo::new(
            asset_id,
            Some(HIDDEN_PUZZLE_HASH),
            SETTLEMENT_PAYMENT_HASH.into(),
        )
        .puzzle_hash()
        .into(),
    )
}

#[derive(Clone, Copy)]
enum Price {
    Xch(u64),
    Cat(u64),
    RevocableCat(u64),
}

impl Price {
    fn amount(self) -> u64 {
        match self {
            Self::Xch(amount) | Self::Cat(amount) | Self::RevocableCat(amount) => amount,
        }
    }
}

struct Tester {
    sim: Simulator,
    ctx: SpendContext,
    next_seed: u64,
    cats: IndexMap<Bytes32, CatAssetInfo>,
}

impl Tester {
    fn new() -> Self {
        Self {
            sim: Simulator::new(),
            ctx: SpendContext::new(),
            next_seed: 100,
            cats: IndexMap::new(),
        }
    }

    fn pair(&mut self) -> BlsPair {
        self.next_seed += 1;
        BlsPair::new(self.next_seed)
    }

    fn build(
        &mut self,
        mut spends: Spends,
        actions: &[Action],
        relation: Relation,
        pair: &BlsPair,
    ) -> anyhow::Result<(Outputs, Vec<CoinSpend>)> {
        let deltas = spends.apply(&mut self.ctx, actions)?;
        let outputs = spends.finish_with_keys(&mut self.ctx, &deltas, relation, &keys(pair))?;
        Ok((outputs, self.ctx.take()))
    }

    fn fund(&mut self, owner: &BlsPair, amount: u64) -> Coin {
        self.sim.new_coin(owner.puzzle_hash, amount)
    }

    fn balance(&self, puzzle_hash: Bytes32) -> u64 {
        self.sim
            .unspent_coins(puzzle_hash, false)
            .iter()
            .map(|coin| coin.amount)
            .sum()
    }

    fn cat_balance(&self, cat: &Cat, p2_puzzle_hash: Bytes32) -> u64 {
        self.balance(cat.child(p2_puzzle_hash, 0).coin.puzzle_hash)
    }

    fn owns_nft(&self, nft: &Nft, p2_puzzle_hash: Bytes32) -> bool {
        let owned = nft.child(p2_puzzle_hash, None, nft.info.metadata, nft.coin.amount);
        self.balance(owned.coin.puzzle_hash) == 1
    }

    fn mint_nft(
        &mut self,
        owner: &BlsPair,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    ) -> anyhow::Result<Nft> {
        let coin = self.fund(owner, 1);
        let mut spends = Spends::new(owner.puzzle_hash);
        spends.add(coin);

        let (outputs, coin_spends) = self.build(
            spends,
            &[Action::mint_empty_royalty_nft(
                royalty_puzzle_hash,
                royalty_basis_points,
            )],
            Relation::None,
            owner,
        )?;

        self.sim
            .spend_coins(coin_spends, slice::from_ref(&owner.sk))?;

        Ok(outputs.nfts[&Id::New(0)])
    }

    fn issue_cat(&mut self, owner: &BlsPair, amount: u64) -> anyhow::Result<Cat> {
        self.issue_cat_with(owner, amount, None)
    }

    fn issue_revocable_cat(&mut self, owner: &BlsPair, amount: u64) -> anyhow::Result<Cat> {
        self.issue_cat_with(owner, amount, Some(HIDDEN_PUZZLE_HASH))
    }

    fn issue_cat_with(
        &mut self,
        owner: &BlsPair,
        amount: u64,
        hidden_puzzle_hash: Option<Bytes32>,
    ) -> anyhow::Result<Cat> {
        let coin = self.fund(owner, amount);
        let hint = self.ctx.hint(owner.puzzle_hash)?;
        let mut spends = Spends::new(owner.puzzle_hash);
        spends.add(coin);

        let (outputs, coin_spends) = self.build(
            spends,
            &[
                Action::single_issue_cat(hidden_puzzle_hash, amount),
                Action::send(Id::New(0), owner.puzzle_hash, amount, hint),
            ],
            Relation::None,
            owner,
        )?;

        self.sim
            .spend_coins(coin_spends, slice::from_ref(&owner.sk))?;

        let cat = outputs.cats[&Id::New(0)]
            .iter()
            .copied()
            .find(|cat| cat.info.p2_puzzle_hash == owner.puzzle_hash && cat.coin.amount == amount)
            .expect("issued cat");

        self.cats
            .insert(cat.info.asset_id, CatAssetInfo::new(hidden_puzzle_hash));

        Ok(cat)
    }

    fn requested_payments(
        &mut self,
        maker: &BlsPair,
        nonce: Bytes32,
        requested: &OfferAmounts,
    ) -> anyhow::Result<(RequestedPayments, AssetInfo)> {
        let hint = self.ctx.hint(maker.puzzle_hash)?;
        let mut requested_payments = RequestedPayments::new();
        let mut asset_info = AssetInfo::new();

        if requested.xch > 0 {
            requested_payments.xch.push(NotarizedPayment::new(
                nonce,
                vec![Payment::new(
                    maker.puzzle_hash,
                    coin_amount(requested.xch)?,
                    hint,
                )],
            ));
        }

        for (&asset_id, &amount) in &requested.cats {
            requested_payments.cats.insert(
                asset_id,
                vec![NotarizedPayment::new(
                    nonce,
                    vec![Payment::new(maker.puzzle_hash, coin_amount(amount)?, hint)],
                )],
            );
            let info = self.cats.get(&asset_id).copied().unwrap_or_default();
            asset_info.insert_cat(asset_id, info)?;
        }

        Ok((requested_payments, asset_info))
    }

    /// Offers NFTs for fungible assets, committing to trade prices the way a wallet would: the
    /// requested amounts are split evenly between NFTs with royalties, leaving out any trade
    /// price whose royalty rounds to zero.
    fn sell_nfts(
        &mut self,
        maker: &BlsPair,
        nfts: &[Nft],
        requested: &OfferAmounts,
    ) -> anyhow::Result<SpendBundle> {
        self.sell_nfts_with(maker, nfts, requested, |nft, trade_prices| {
            payable_trade_prices(trade_prices, nft.info.royalty_basis_points)
        })
    }

    fn sell_nfts_with(
        &mut self,
        maker: &BlsPair,
        nfts: &[Nft],
        requested: &OfferAmounts,
        trade_prices_for: impl Fn(&Nft, &[TradePrice]) -> Vec<TradePrice>,
    ) -> anyhow::Result<SpendBundle> {
        let nonce = Offer::nonce(nfts.iter().map(|nft| nft.coin.coin_id()).collect());
        let (requested_payments, asset_info) = self.requested_payments(maker, nonce, requested)?;

        let royalty_nft_count = nfts
            .iter()
            .filter(|nft| nft.info.royalty_basis_points > 0)
            .count();
        let trade_prices = calculate_trade_prices(
            &calculate_trade_price_amounts(requested, royalty_nft_count),
            &asset_info,
        )?;

        let mut spends = Spends::new(maker.puzzle_hash);
        let mut actions = Vec::new();

        for nft in nfts {
            let id = Id::Existing(nft.info.launcher_id);
            spends.add(*nft);
            actions.push(Action::update_nft(
                id,
                vec![],
                Some(TransferNftById::new(
                    None,
                    trade_prices_for(nft, &trade_prices),
                )),
            ));
            actions.push(Action::send(
                id,
                SETTLEMENT_PAYMENT_HASH.into(),
                1,
                Memos::None,
            ));
        }

        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut self.ctx, &asset_info)?);

        let (_, coin_spends) = self.build(spends, &actions, Relation::AssertConcurrent, maker)?;
        self.finish_offer(maker, coin_spends, requested_payments, asset_info)
    }

    /// Offers fungible assets for an NFT, paying its royalty on the offered price up front.
    fn bid(&mut self, maker: &BlsPair, nft: &Nft, price: Price) -> anyhow::Result<SpendBundle> {
        self.bid_in_assets(maker, nft, &[price])
    }

    /// Offers fungible assets for an NFT, paying its royalty on each offered price up front.
    fn bid_in_assets(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[Price],
    ) -> anyhow::Result<SpendBundle> {
        let mut with_royalties = Vec::new();

        for &price in prices {
            let royalty = calculate_nft_royalty(price.amount(), nft.info.royalty_basis_points);
            with_royalties.push((price, coin_amount(royalty)?));
        }

        self.bid_with_royalties(maker, nft, &with_royalties)
    }

    fn bid_with_royalty(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        price: Price,
        royalty: u64,
    ) -> anyhow::Result<SpendBundle> {
        self.bid_with_royalties(maker, nft, &[(price, royalty)])
    }

    fn bid_with_royalties(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[(Price, u64)],
    ) -> anyhow::Result<SpendBundle> {
        let launcher_id = nft.info.launcher_id;
        let mut spends = Spends::new(maker.puzzle_hash);
        let mut actions = Vec::new();
        let mut coin_ids = Vec::new();

        for &(price, royalty) in prices {
            let total = price.amount() + royalty;

            let (id, coin_id) = match price {
                Price::Xch(_) => {
                    let coin = self.fund(maker, total);
                    spends.add(coin);
                    (Id::Xch, coin.coin_id())
                }
                Price::Cat(_) => {
                    let cat = self.issue_cat(maker, total)?;
                    spends.add(cat);
                    (Id::Existing(cat.info.asset_id), cat.coin.coin_id())
                }
                Price::RevocableCat(_) => {
                    let cat = self.issue_revocable_cat(maker, total)?;
                    spends.add(cat);
                    (Id::Existing(cat.info.asset_id), cat.coin.coin_id())
                }
            };

            coin_ids.push(coin_id);
            actions.push(Action::send(
                id,
                SETTLEMENT_PAYMENT_HASH.into(),
                price.amount(),
                Memos::None,
            ));

            if royalty > 0 {
                actions.push(Action::settle_royalty(
                    &mut self.ctx,
                    id,
                    launcher_id,
                    nft.info.royalty_puzzle_hash,
                    royalty,
                )?);
            }
        }

        let nonce = Offer::nonce(coin_ids);
        let hint = self.ctx.hint(maker.puzzle_hash)?;
        let mut requested_payments = RequestedPayments::new();
        requested_payments.nfts.insert(
            launcher_id,
            vec![NotarizedPayment::new(
                nonce,
                vec![Payment::new(maker.puzzle_hash, 1, hint)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_nft(
            launcher_id,
            NftAssetInfo::new(
                nft.info.metadata,
                nft.info.metadata_updater_puzzle_hash,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            ),
        )?;

        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut self.ctx, &asset_info)?);

        let (_, coin_spends) = self.build(spends, &actions, Relation::AssertConcurrent, maker)?;
        self.finish_offer(maker, coin_spends, requested_payments, asset_info)
    }

    /// Offers XCH for a CAT, without any NFTs involved.
    fn trade_xch_for_cat(
        &mut self,
        maker: &BlsPair,
        offered: u64,
        requested_asset_id: Bytes32,
        requested: u64,
    ) -> anyhow::Result<SpendBundle> {
        let coin = self.fund(maker, offered);
        let (requested_payments, asset_info) =
            self.requested_payments(maker, coin.coin_id(), &cat(requested_asset_id, requested))?;

        let mut spends = Spends::new(maker.puzzle_hash);
        spends.add(coin);
        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut self.ctx, &asset_info)?);

        let (_, coin_spends) = self.build(
            spends,
            &[Action::send(
                Id::Xch,
                SETTLEMENT_PAYMENT_HASH.into(),
                offered,
                Memos::None,
            )],
            Relation::AssertConcurrent,
            maker,
        )?;
        self.finish_offer(maker, coin_spends, requested_payments, asset_info)
    }

    fn finish_offer(
        &mut self,
        maker: &BlsPair,
        coin_spends: Vec<CoinSpend>,
        requested_payments: RequestedPayments,
        asset_info: AssetInfo,
    ) -> anyhow::Result<SpendBundle> {
        let signature = sign_transaction(&coin_spends, slice::from_ref(&maker.sk))?;
        let offer = Offer::from_input_spend_bundle(
            &mut self.ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            asset_info,
        )?;
        Ok(offer.to_spend_bundle(&mut self.ctx)?)
    }

    fn aggregate(&mut self, spend_bundles: &[SpendBundle]) -> anyhow::Result<Offer> {
        Ok(Offer::from_spend_bundle(
            &mut self.ctx,
            &SpendBundle::aggregate(spend_bundles),
        )?)
    }

    /// Takes the offer, paying the royalties and revealing the trade prices the SDK derives.
    fn take(
        &mut self,
        taker: &BlsPair,
        offer: &Offer,
        add: impl FnOnce(&mut Spends),
    ) -> anyhow::Result<Outputs> {
        let royalties = offer.requested_royalty_payments(&mut self.ctx)?.actions();
        self.take_with(taker, offer, add, royalties, |offer, launcher_id| {
            offer.requested_nft_trade_prices(launcher_id)
        })
    }

    fn take_with(
        &mut self,
        taker: &BlsPair,
        offer: &Offer,
        add: impl FnOnce(&mut Spends),
        royalty_actions: Vec<Action>,
        trade_prices_for: impl Fn(&Offer, Bytes32) -> Vec<TradePrice>,
    ) -> anyhow::Result<Outputs> {
        let mut spends = Spends::new(taker.puzzle_hash);
        spends.add(offer.offered_coins().clone());
        add(&mut spends);

        let mut actions = Vec::new();

        for &launcher_id in offer.requested_payments().nfts.keys() {
            // Offered NFTs only pass through settlement, so they don't reveal trade prices
            if offer.offered_coins().nfts.contains_key(&launcher_id) {
                continue;
            }

            actions.push(Action::update_nft(
                Id::Existing(launcher_id),
                vec![],
                Some(TransferNftById::new(
                    None,
                    trade_prices_for(offer, launcher_id),
                )),
            ));
        }

        actions.extend(offer.requested_payments().actions());
        actions.extend(royalty_actions);

        let (outputs, coin_spends) =
            self.build(spends, &actions, Relation::AssertConcurrent, taker)?;
        let signature = sign_transaction(&coin_spends, slice::from_ref(&taker.sk))?;

        self.sim
            .new_transaction(offer.clone().take(SpendBundle::new(coin_spends, signature)))?;

        Ok(outputs)
    }

    fn royalty(
        &mut self,
        id: Id,
        nft: &Nft,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> anyhow::Result<Action> {
        Ok(Action::settle_royalty(
            &mut self.ctx,
            id,
            nft.info.launcher_id,
            puzzle_hash,
            amount,
        )?)
    }
}

#[test]
fn test_payable_trade_prices() {
    let asset_id = Bytes32::new([9; 32]);
    let trade_prices = [
        xch_trade_price(10),
        xch_trade_price(34),
        cat_trade_price(asset_id, 1000),
    ];

    // 3% of 10 rounds down to zero, but 3% of 34 is 1
    assert_eq!(
        payable_trade_prices(&trade_prices, 300),
        vec![xch_trade_price(34), cat_trade_price(asset_id, 1000)]
    );
    assert_eq!(payable_trade_prices(&trade_prices, 0), Vec::new());
    assert_eq!(payable_trade_prices(&[], 300), Vec::new());
}

#[test]
fn test_prepaid_royalties_from_untrusted_offer() -> anyhow::Result<()> {
    let launcher_id = Bytes32::new([4; 32]);
    let hint = Memos::None;

    let mut offered_coins = OfferCoins::new();
    offered_coins.settled_payments.xch = vec![
        NotarizedPayment::new(
            launcher_id,
            vec![
                Payment::new(ROYALTY_A, u64::MAX, hint),
                Payment::new(ROYALTY_B, 1000, hint),
            ],
        ),
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, u64::MAX, hint)]),
        NotarizedPayment::new(
            Bytes32::new([5; 32]),
            vec![Payment::new(ROYALTY_A, 1000, hint)],
        ),
    ];

    // A requested NFT without asset info has no known royalty address
    let mut requested_payments = RequestedPayments::new();
    for id in [launcher_id, Bytes32::new([6; 32])] {
        requested_payments.nfts.insert(
            id,
            vec![NotarizedPayment::new(
                Bytes32::default(),
                vec![Payment::new(ROYALTY_C, 1, hint)],
            )],
        );
    }

    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        launcher_id,
        NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, 1),
    )?;

    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins,
        requested_payments,
        asset_info,
    );

    // Only payments to the royalty address with the NFT's nonce count, and the total can exceed a
    // u64
    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 2 * u128::from(u64::MAX),
            cats: IndexMap::new(),
        }
    );

    // No trade price is large enough to produce these royalties
    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());
    assert_eq!(
        offer.requested_nft_trade_prices(Bytes32::new([6; 32])),
        Vec::new()
    );

    Ok(())
}

#[test]
fn test_unparseable_settled_payments_are_ignored() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let settlement = t.ctx.alloc_mod::<SettlementPayment>()?;

    // Settlement accepts a nonce that isn't 32 bytes, but it can't be parsed as a payment
    let solution = t.ctx.alloc(&clvm_list!(clvm_list!(1)))?;
    let xch_spend = CoinSpend::new(
        Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 1),
        t.ctx.serialize(&settlement)?,
        t.ctx.serialize(&solution)?,
    );

    let cat = t
        .issue_cat(&alice, 1)?
        .child(SETTLEMENT_PAYMENT_HASH.into(), 1);
    let solution = t
        .ctx
        .alloc(&clvm_list!(clvm_list!(1, clvm_list!(ROYALTY_A, 1))))?;
    Cat::spend_all(
        &mut t.ctx,
        &[CatSpend::new(cat, Spend::new(settlement, solution))],
    )?;

    let mut coin_spends = t.ctx.take();
    coin_spends.push(xch_spend);
    let offer = Offer::from_spend_bundle(
        &mut t.ctx,
        &SpendBundle::new(coin_spends, Signature::default()),
    )?;

    assert!(offer.offered_coins().settled_payments.xch.is_empty());
    assert!(offer.offered_coins().settled_payments.cats.is_empty());

    Ok(())
}

#[test]
fn test_invalid_nft_spends_are_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let raise = t.ctx.alloc(&clvm_list!(8))?;

    let spend_nft = |ctx: &mut SpendContext, inner_puzzle| -> anyhow::Result<()> {
        let spend = nft.info.into_layers(inner_puzzle).construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: nft.proof,
                amount: nft.coin.amount,
                inner_solution: NftStateLayerSolution {
                    inner_solution: NftOwnershipLayerSolution {
                        inner_solution: NodePtr::NIL,
                    },
                },
            },
        )?;
        ctx.spend(nft.coin, spend)?;
        Ok(())
    };

    // The NFT's solution doesn't match its layers
    spend_nft(&mut t.ctx, raise)?;
    let mut coin_spends = t.ctx.take();
    coin_spends[0].solution = t.ctx.serialize(&1)?;
    assert!(
        Offer::from_spend_bundle(
            &mut t.ctx,
            &SpendBundle::new(coin_spends, Signature::default())
        )
        .is_err()
    );

    // The NFT's inner puzzle raises
    spend_nft(&mut t.ctx, raise)?;
    let coin_spends = t.ctx.take();
    assert!(
        Offer::from_spend_bundle(
            &mut t.ctx,
            &SpendBundle::new(coin_spends, Signature::default())
        )
        .is_err()
    );

    Ok(())
}

#[test]
fn test_offered_nft_without_known_trade_prices() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    let mut offered_coins = offer.offered_coins().clone();
    offered_coins.nft_trade_prices.clear();
    let offer = Offer::new(
        offer.spend_bundle().clone(),
        offered_coins,
        offer.requested_payments().clone(),
        offer.asset_info().clone(),
    );

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert!(
        offer
            .requested_royalty_payments(&mut t.ctx)?
            .actions()
            .is_empty()
    );

    Ok(())
}

#[test]
fn test_royalty_larger_than_a_coin_is_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    // Royalties above 100% are allowed, so a trade price that fits in a coin can have a royalty
    // that doesn't
    let nft = t.mint_nft(&alice, ROYALTY_A, u16::MAX)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1), |_, _| {
        vec![xch_trade_price(u64::MAX)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: u128::from(u64::MAX) * 65_535 / 10_000,
            cats: IndexMap::new(),
        }
    );
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::AmountOverflow)
    ));

    Ok(())
}

#[test]
fn test_sell_nft_for_xch() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(30));

    let coin = t.fund(&bob, 1030);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_royalty_rounds_down() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // 3% of 1999 is 59.97
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1999))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(59));

    let coin = t.fund(&bob, 1999 + 59);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 59);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_royalty_must_match_exactly() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;
    let other_nonce = Bytes32::new([7; 32]);

    let mut wrong_royalties = Vec::new();

    for amount in [29, 31] {
        wrong_royalties.push(t.royalty(Id::Xch, &nft, ROYALTY_A, amount)?);
    }

    wrong_royalties.push(t.royalty(Id::Xch, &nft, ROYALTY_B, 30)?);

    let hint = t.ctx.hint(ROYALTY_A)?;
    wrong_royalties.push(Action::settle(
        Id::Xch,
        NotarizedPayment::new(other_nonce, vec![Payment::new(ROYALTY_A, 30, hint)]),
    ));
    wrong_royalties.push(Action::settle(
        Id::Xch,
        NotarizedPayment::new(
            nft.info.launcher_id,
            vec![Payment::new(ROYALTY_A, 30, Memos::None)],
        ),
    ));

    for royalty in wrong_royalties {
        let coin = t.fund(&bob, 1031);
        let error = t
            .take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                vec![royalty],
                |_, _| Vec::new(),
            )
            .expect_err("wrong royalty payment should be rejected");

        assert!(
            is_validation_error(&error, ErrorCode::AssertPuzzleAnnouncementFailed),
            "{error}"
        );
    }

    let coin = t.fund(&bob, 1030);
    let error = t
        .take_with(
            &bob,
            &offer,
            |spends| spends.add(coin),
            Vec::new(),
            |_, _| Vec::new(),
        )
        .expect_err("missing royalty payment should be rejected");

    assert!(is_validation_error(
        &error,
        ErrorCode::AssertPuzzleAnnouncementFailed
    ));

    Ok(())
}

#[test]
fn test_royalty_must_be_paid_in_trade_price_asset() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 1000)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    let coin = t.fund(&bob, 30);
    let royalty = t.royalty(Id::Xch, &nft, ROYALTY_A, 30)?;
    let error = t
        .take_with(
            &bob,
            &offer,
            |spends| {
                spends.add(coin);
                spends.add(bob_cat);
            },
            vec![royalty],
            |_, _| Vec::new(),
        )
        .expect_err("royalty paid in the wrong asset should be rejected");

    assert!(is_validation_error(
        &error,
        ErrorCode::AssertPuzzleAnnouncementFailed
    ));

    Ok(())
}

#[test]
fn test_sell_nft_for_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 1030)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, cat(asset_id, 30));

    t.take(&bob, &offer, |spends| spends.add(bob_cat))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.cat_balance(&bob_cat, alice.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_sell_nft_for_xch_and_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 515)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let requested = OfferAmounts {
        xch: 1000,
        cats: indexmap! { asset_id => 500 },
    };
    let offer = t.sell_nfts(&alice, &[nft], &requested)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: 30,
            cats: indexmap! { asset_id => 15 },
        }
    );

    let coin = t.fund(&bob, 1030);
    t.take(&bob, &offer, |spends| {
        spends.add(coin);
        spends.add(bob_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 15);
    assert_eq!(t.balance(bob.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_each_trade_price_is_paid_separately() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 1000)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(400), |_, _| {
        vec![xch_trade_price(100), xch_trade_price(300)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(40));

    let coin = t.fund(&bob, 440);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    let royalty_coins = t.sim.unspent_coins(ROYALTY_A, false);
    let mut royalty_amounts: Vec<u64> = royalty_coins.iter().map(|coin| coin.amount).collect();
    royalty_amounts.sort_unstable();
    assert_eq!(royalty_amounts, vec![10, 30]);

    Ok(())
}

#[test]
fn test_identical_royalty_payments_are_paid_once() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // 1% of both 100 and 101 is 1, so the NFT asserts the same announcement twice
    let nft = t.mint_nft(&alice, ROYALTY_A, 100)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(201), |_, _| {
        vec![xch_trade_price(100), xch_trade_price(101)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(1));

    let coin = t.fund(&bob, 202);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 1);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_nft_for_nft_has_no_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;

    let nonce = Offer::nonce(vec![alice_nft.coin.coin_id()]);
    let hint = t.ctx.hint(alice.puzzle_hash)?;
    let mut requested_payments = RequestedPayments::new();
    requested_payments.nfts.insert(
        bob_nft.info.launcher_id,
        vec![NotarizedPayment::new(
            nonce,
            vec![Payment::new(alice.puzzle_hash, 1, hint)],
        )],
    );
    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        bob_nft.info.launcher_id,
        NftAssetInfo::new(
            bob_nft.info.metadata,
            bob_nft.info.metadata_updater_puzzle_hash,
            bob_nft.info.royalty_puzzle_hash,
            bob_nft.info.royalty_basis_points,
        ),
    )?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice_nft);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut t.ctx, &asset_info)?);
    let id = Id::Existing(alice_nft.info.launcher_id);
    let (_, coin_spends) = t.build(
        spends,
        &[
            Action::update_nft(id, vec![], Some(TransferNftById::new(None, Vec::new()))),
            Action::send(id, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
        ],
        Relation::AssertConcurrent,
        &alice,
    )?;
    let offer = t.finish_offer(&alice, coin_spends, requested_payments, asset_info)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(offer.offered_royalty_amounts()?, OfferAmounts::new());
    assert!(
        offer
            .requested_royalty_payments(&mut t.ctx)?
            .actions()
            .is_empty()
    );
    assert_eq!(
        offer.requested_nft_trade_prices(bob_nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(bob_nft))?;

    assert!(t.owns_nft(&alice_nft, bob.puzzle_hash));
    assert!(t.owns_nft(&bob_nft, alice.puzzle_hash));

    Ok(())
}

#[test]
fn test_royalty_free_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 0)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    let coin = t.fund(&bob, 1000);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 0);

    Ok(())
}

#[test]
fn test_zero_royalty_trade_price_is_unpayable() -> anyhow::Result<()> {
    for (basis_points, price) in [(300, 10), (0, 1000)] {
        let mut t = Tester::new();
        let alice = t.pair();
        let bob = t.pair();

        let nft = t.mint_nft(&alice, ROYALTY_A, basis_points)?;
        let offer = t.sell_nfts_with(&alice, &[nft], &xch(price), |_, _| {
            vec![xch_trade_price(price)]
        })?;
        let offer = t.aggregate(&[offer])?;

        assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
        assert!(matches!(
            offer.requested_royalty_payments(&mut t.ctx),
            Err(DriverError::ZeroRoyaltyPayment(launcher_id)) if launcher_id == nft.info.launcher_id
        ));

        // The NFT asserts a payment of zero, which settlement refuses to make
        let coin = t.fund(&bob, price);
        let hint = t.ctx.hint(ROYALTY_A)?;
        let zero_royalty = Action::settle(
            Id::Xch,
            NotarizedPayment::new(nft.info.launcher_id, vec![Payment::new(ROYALTY_A, 0, hint)]),
        );
        assert!(
            t.take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                vec![zero_royalty],
                |_, _| Vec::new(),
            )
            .is_err()
        );

        let coin = t.fund(&bob, price);
        let error = t
            .take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                Vec::new(),
                |_, _| Vec::new(),
            )
            .expect_err("the royalty announcement can never be made");
        assert!(is_validation_error(
            &error,
            ErrorCode::AssertPuzzleAnnouncementFailed
        ));
    }

    Ok(())
}

#[test]
fn test_payable_trade_prices_make_small_sales_takeable() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(10))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    let coin = t.fund(&bob, 10);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 10);

    Ok(())
}

#[test]
fn test_unknown_trade_price_asset_is_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let unknown_asset_id = Bytes32::new([8; 32]);
    let unknown = cat_trade_price(unknown_asset_id, 1000);
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1000), |_, _| vec![unknown])?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::UnknownTradePriceAsset(puzzle_hash)) if puzzle_hash == unknown.puzzle_hash
    ));

    Ok(())
}

#[test]
fn test_aggregate_sells_same_price_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 1000)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(1500));

    let coin = t.fund(&carol, 21_500);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_different_prices_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 1000)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(30_000))?;
    let offer = t.aggregate(&[alice_offer.clone(), bob_offer.clone()])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 3000));

    // Extending parsed offers keeps each maker's trade prices too
    let mut extended = t.aggregate(&[alice_offer])?;
    extended.extend(t.aggregate(&[bob_offer])?)?;
    assert_eq!(extended.requested_royalty_amounts()?, xch(500 + 3000));

    let coin = t.fund(&carol, 43_500);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_different_prices_same_royalty() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(30_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 1500));

    let coin = t.fund(&carol, 42_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_multi_nft_offer_with_single_nft_offer() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Alice's two NFTs each commit to half of her price
    let first_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&alice, ROYALTY_B, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_C, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[first_nft, second_nft], &xch(30_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(750 + 750 + 500));

    let coin = t.fund(&carol, 42_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 750);
    assert_eq!(t.balance(ROYALTY_B), 750);
    assert_eq!(t.balance(ROYALTY_C), 500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_for_different_assets() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let carol_cat = t.issue_cat(&carol, 10_500)?;
    let asset_id = carol_cat.info.asset_id;
    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &cat(asset_id, 10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: 500,
            cats: indexmap! { asset_id => 500 },
        }
    );

    let coin = t.fund(&carol, 10_500);
    t.take(&carol, &offer, |spends| {
        spends.add(coin);
        spends.add(carol_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_A), 0);
    assert_eq!(t.balance(ROYALTY_B), 0);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_B), 500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&carol_cat, carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_nft_sale_with_unrelated_cat_trade() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let carol_cat = t.issue_cat(&carol, 21_000)?;
    let asset_id = carol_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(10_000))?;
    let bob_offer = t.trade_xch_for_cat(&bob, 5000, asset_id, 20_000)?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    // Only Alice's NFT is sold, and only for XCH
    assert_eq!(offer.requested_royalty_amounts()?, xch(500));

    let coin = t.fund(&carol, 5500);
    t.take(&carol, &offer, |spends| {
        spends.add(coin);
        spends.add(carol_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_A), 0);
    assert_eq!(t.balance(carol.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&carol_cat, carol.puzzle_hash), 1000);

    Ok(())
}

#[test]
fn test_bid_for_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // 3% of 1999 is 59, which Alice pays up front
    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::Xch(1999))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(59));
    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    // The smallest trade price with a 59 royalty
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1967)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1999);
    assert_eq!(t.balance(ROYALTY_A), 59);

    Ok(())
}

#[test]
fn test_bid_for_nft_with_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::Cat(1000))?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(offer.offered_royalty_amounts()?, cat(asset_id, 30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![cat_trade_price(asset_id, 1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_aggregate_bids_same_price_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 1000));

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert!(t.owns_nft(&first_nft, alice.puzzle_hash));
    assert!(t.owns_nft(&second_nft, bob.puzzle_hash));
    assert_eq!(t.balance(carol.puzzle_hash), 20_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1000);

    Ok(())
}

#[test]
fn test_aggregate_bids_different_prices() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(30_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 3000));
    assert_eq!(
        offer.requested_nft_trade_prices(first_nft.info.launcher_id),
        vec![xch_trade_price(10_000)]
    );
    assert_eq!(
        offer.requested_nft_trade_prices(second_nft.info.launcher_id),
        vec![xch_trade_price(30_000)]
    );

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert_eq!(t.balance(carol.puzzle_hash), 40_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);

    Ok(())
}

#[test]
fn test_unmatchable_prepaid_royalty_is_not_revealed() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // At 200%, every royalty is even, so a prepaid royalty of 3 has no matching trade price
    let nft = t.mint_nft(&bob, ROYALTY_A, 20_000)?;
    let offer = t.bid_with_royalty(&alice, &nft, Price::Xch(1000), 3)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(3));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 3);

    Ok(())
}

#[test]
fn test_prepaid_royalties_for_royalty_free_nft_are_not_revealed() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // Alice still pays the royalty, even though the NFT can't assert it
    let nft = t.mint_nft(&bob, ROYALTY_A, 0)?;
    let offer = t.bid_with_royalty(&alice, &nft, Price::Xch(1000), 5)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(5));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 5);

    Ok(())
}

#[test]
fn test_aggregate_sale_and_bid_for_same_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // The NFT passes through settlement from Alice to Bob, so Carol never owns it
    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let bob_offer = t.bid(&bob, &nft, Price::Xch(1500))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(50));
    assert_eq!(offer.offered_royalty_amounts()?, xch(75));

    t.take(&carol, &offer, |_| {})?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 450);
    assert_eq!(t.balance(ROYALTY_A), 50 + 75);

    Ok(())
}

#[test]
fn test_sell_nft_for_revocable_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // The hidden puzzle hash changes the CAT's settlement puzzle hash
    let bob_cat = t.issue_revocable_cat(&bob, 1030)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, cat(asset_id, 30));

    t.take(&bob, &offer, |spends| spends.add(bob_cat))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.cat_balance(&bob_cat, alice.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_bid_for_nft_with_revocable_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::RevocableCat(1000))?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(offer.offered_royalty_amounts()?, cat(asset_id, 30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![revocable_cat_trade_price(asset_id, 1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_aggregate_identical_royalties_for_different_nfts() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Both NFTs pay the same royalty to the same address, but each asserts its own payment
    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 500));

    let coin = t.fund(&carol, 21_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&alice_nft, carol.puzzle_hash));
    assert!(t.owns_nft(&bob_nft, carol.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_extend_bids() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(30_000))?;

    // Extending parsed offers keeps each bidder's prepaid royalties
    let mut offer = t.aggregate(&[alice_offer])?;
    offer.extend(t.aggregate(&[bob_offer])?)?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 3000));
    assert_eq!(
        offer.requested_nft_trade_prices(first_nft.info.launcher_id),
        vec![xch_trade_price(10_000)]
    );
    assert_eq!(
        offer.requested_nft_trade_prices(second_nft.info.launcher_id),
        vec![xch_trade_price(30_000)]
    );

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert!(t.owns_nft(&first_nft, alice.puzzle_hash));
    assert!(t.owns_nft(&second_nft, bob.puzzle_hash));
    assert_eq!(t.balance(carol.puzzle_hash), 40_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);

    Ok(())
}

#[test]
fn test_bid_for_nft_in_two_assets() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // Alice prepays a royalty in each asset, so the NFT reveals a trade price for each
    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid_in_assets(&alice, &nft, &[Price::Xch(1000), Price::Cat(2000)])?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 30,
            cats: indexmap! { asset_id => 60 },
        }
    );
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1000), cat_trade_price(asset_id, 2000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 2000);
    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 60);

    Ok(())
}

#[test]
fn test_smallest_trade_price_for_prepaid_royalty() -> anyhow::Result<()> {
    let launcher_id = Bytes32::new([4; 32]);

    let trade_prices = |basis_points: u16, royalty: u64| -> anyhow::Result<Vec<TradePrice>> {
        let mut offered_coins = OfferCoins::new();
        offered_coins.settled_payments.xch = vec![NotarizedPayment::new(
            launcher_id,
            vec![Payment::new(ROYALTY_A, royalty, Memos::None)],
        )];

        let mut requested_payments = RequestedPayments::new();
        requested_payments.nfts.insert(launcher_id, Vec::new());

        let mut asset_info = AssetInfo::new();
        asset_info.insert_nft(
            launcher_id,
            NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, basis_points),
        )?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        Ok(offer.requested_nft_trade_prices(launcher_id))
    };

    // Exact, rounded, 100%, and above 100%
    assert_eq!(trade_prices(300, 30)?, vec![xch_trade_price(1000)]);
    assert_eq!(trade_prices(300, 31)?, vec![xch_trade_price(1034)]);
    assert_eq!(trade_prices(10_000, 7)?, vec![xch_trade_price(7)]);
    assert_eq!(trade_prices(20_000, 8)?, vec![xch_trade_price(4)]);

    // At 200% every royalty is even, and a 0.01% royalty of u64::MAX needs a larger trade price
    assert_eq!(trade_prices(20_000, 7)?, Vec::new());
    assert_eq!(trade_prices(1, u64::MAX)?, Vec::new());

    // Otherwise, the trade price is the smallest one that produces exactly the royalty
    for basis_points in [1, 3, 250, 300, 9999, 10_000, 10_001, 65_535] {
        for royalty in (1..=200).chain([u64::MAX / 7, u64::MAX]) {
            let trade_prices = trade_prices(basis_points, royalty)?;

            // Up to 100%, each unit of trade price adds at most one unit of royalty
            if basis_points <= 10_000 && royalty <= 200 {
                assert_eq!(trade_prices.len(), 1);
            }

            for trade_price in trade_prices {
                let amount = trade_price.amount;
                assert_eq!(
                    calculate_nft_royalty(amount, basis_points),
                    u128::from(royalty)
                );
                assert!(
                    amount == 0 || calculate_nft_royalty(amount - 1, basis_points) < royalty.into()
                );
            }
        }
    }

    Ok(())
}
