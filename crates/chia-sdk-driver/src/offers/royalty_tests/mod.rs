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

mod bids;
mod combined_sales;
mod puzzle_rules;
mod untrusted_input;

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
