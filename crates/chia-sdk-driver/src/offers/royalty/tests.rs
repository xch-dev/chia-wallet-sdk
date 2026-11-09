use std::slice;

use anyhow::Result;
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
use rstest::rstest;

use crate::{
    Action, AssetInfo, Cat, CatAssetInfo, CatInfo, CatSpend, DriverError, HashedPtr, Id, Layer,
    Nft, NftAssetInfo, Offer, OfferAmounts, OfferCoins, Outputs, Relation, RequestedPayments,
    RoyaltyInfo, Spend, SpendContext, Spends, TransferNftById, calculate_nft_royalty,
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

/// Coins of the asset a [`Price`] is paid in.
#[derive(Clone, Copy)]
enum Funds {
    Xch(Coin),
    Cat(Cat),
}

impl Funds {
    fn amounts(self, amount: u64) -> OfferAmounts {
        match self {
            Self::Xch(_) => xch(amount),
            Self::Cat(funds) => cat(funds.info.asset_id, amount),
        }
    }

    fn add_to(self, spends: &mut Spends) {
        match self {
            Self::Xch(coin) => spends.add(coin),
            Self::Cat(funds) => spends.add(funds),
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
    ) -> Result<(Outputs, Vec<CoinSpend>)> {
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

    /// Funds `owner` with `price` in the asset it's paid in.
    fn fund_in(&mut self, owner: &BlsPair, price: Price) -> Result<Funds> {
        Ok(match price {
            Price::Xch(amount) => Funds::Xch(self.fund(owner, amount)),
            Price::Cat(amount) => Funds::Cat(self.issue_cat(owner, amount)?),
            Price::RevocableCat(amount) => Funds::Cat(self.issue_revocable_cat(owner, amount)?),
        })
    }

    fn funds_balance(&self, funds: Funds, p2_puzzle_hash: Bytes32) -> u64 {
        match funds {
            Funds::Xch(_) => self.balance(p2_puzzle_hash),
            Funds::Cat(funds) => self.cat_balance(&funds, p2_puzzle_hash),
        }
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
    ) -> Result<Nft> {
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

    fn issue_cat(&mut self, owner: &BlsPair, amount: u64) -> Result<Cat> {
        self.issue_cat_with(owner, amount, None)
    }

    fn issue_revocable_cat(&mut self, owner: &BlsPair, amount: u64) -> Result<Cat> {
        self.issue_cat_with(owner, amount, Some(HIDDEN_PUZZLE_HASH))
    }

    fn issue_cat_with(
        &mut self,
        owner: &BlsPair,
        amount: u64,
        hidden_puzzle_hash: Option<Bytes32>,
    ) -> Result<Cat> {
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
    ) -> Result<(RequestedPayments, AssetInfo)> {
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

    fn sell_nfts(
        &mut self,
        maker: &BlsPair,
        nfts: &[Nft],
        requested: &OfferAmounts,
    ) -> Result<SpendBundle> {
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
    ) -> Result<SpendBundle> {
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

    /// Offers `offered` for `requested` with no fungible assets on either side, so neither NFT
    /// commits to a trade price.
    fn swap_nfts(
        &mut self,
        maker: &BlsPair,
        offered: &Nft,
        requested: &Nft,
    ) -> Result<SpendBundle> {
        let nonce = Offer::nonce(vec![offered.coin.coin_id()]);
        let hint = self.ctx.hint(maker.puzzle_hash)?;
        let mut requested_payments = RequestedPayments::new();
        requested_payments.nfts.insert(
            requested.info.launcher_id,
            vec![NotarizedPayment::new(
                nonce,
                vec![Payment::new(maker.puzzle_hash, 1, hint)],
            )],
        );
        let mut asset_info = AssetInfo::new();
        asset_info.insert_nft(
            requested.info.launcher_id,
            NftAssetInfo::new(
                requested.info.metadata,
                requested.info.metadata_updater_puzzle_hash,
                requested.info.royalty_puzzle_hash,
                requested.info.royalty_basis_points,
            ),
        )?;

        let mut spends = Spends::new(maker.puzzle_hash);
        spends.add(*offered);
        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut self.ctx, &asset_info)?);
        let id = Id::Existing(offered.info.launcher_id);
        let (_, coin_spends) = self.build(
            spends,
            &[
                Action::update_nft(id, vec![], Some(TransferNftById::new(None, Vec::new()))),
                Action::send(id, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
            ],
            Relation::AssertConcurrent,
            maker,
        )?;
        self.finish_offer(maker, coin_spends, requested_payments, asset_info)
    }

    fn bid(&mut self, maker: &BlsPair, nft: &Nft, price: Price) -> Result<SpendBundle> {
        self.bid_in_assets(maker, nft, &[price])
    }

    fn bid_in_assets(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[Price],
    ) -> Result<SpendBundle> {
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
    ) -> Result<SpendBundle> {
        self.bid_with_royalties(maker, nft, &[(price, royalty)])
    }

    fn bid_with_royalties(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[(Price, u64)],
    ) -> Result<SpendBundle> {
        let royalty = RoyaltyInfo::new(
            nft.info.launcher_id,
            nft.info.royalty_puzzle_hash,
            nft.info.royalty_basis_points,
        );
        let mut with_settlements = Vec::new();

        for &(price, amount) in prices {
            let settlements = if amount > 0 {
                vec![royalty.payment(&mut self.ctx, amount)?]
            } else {
                Vec::new()
            };
            with_settlements.push((price, settlements));
        }

        self.bid_with_settlements(maker, nft, &with_settlements)
    }

    fn bid_with_settlements(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[(Price, Vec<NotarizedPayment>)],
    ) -> Result<SpendBundle> {
        self.bid_with_settlements_and_request(maker, nft, prices, &OfferAmounts::new())
    }

    /// Bids for `nft` and also requests `requested` fungible amounts alongside it.
    fn bid_with_settlements_and_request(
        &mut self,
        maker: &BlsPair,
        nft: &Nft,
        prices: &[(Price, Vec<NotarizedPayment>)],
        requested: &OfferAmounts,
    ) -> Result<SpendBundle> {
        let launcher_id = nft.info.launcher_id;
        let mut spends = Spends::new(maker.puzzle_hash);
        let mut actions = Vec::new();
        let mut coin_ids = Vec::new();

        for (price, settlements) in prices {
            let price = *price;
            let total = price.amount()
                + settlements
                    .iter()
                    .flat_map(|settlement| &settlement.payments)
                    .map(|payment| payment.amount)
                    .sum::<u64>();

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

            for settlement in settlements {
                actions.push(Action::settle(id, settlement.clone()));
            }
        }

        let nonce = Offer::nonce(coin_ids);
        let hint = self.ctx.hint(maker.puzzle_hash)?;
        let (mut requested_payments, mut asset_info) =
            self.requested_payments(maker, nonce, requested)?;
        requested_payments.nfts.insert(
            launcher_id,
            vec![NotarizedPayment::new(
                nonce,
                vec![Payment::new(maker.puzzle_hash, 1, hint)],
            )],
        );

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

    fn trade_xch_for_cat(
        &mut self,
        maker: &BlsPair,
        offered: u64,
        requested_asset_id: Bytes32,
        requested: u64,
    ) -> Result<SpendBundle> {
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
    ) -> Result<SpendBundle> {
        let signature = sign_transaction(&coin_spends, slice::from_ref(&maker.sk))?;
        let offer = Offer::from_input_spend_bundle(
            &mut self.ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            asset_info,
        )?;
        Ok(offer.to_spend_bundle(&mut self.ctx)?)
    }

    fn aggregate(&mut self, spend_bundles: &[SpendBundle]) -> Result<Offer> {
        Ok(Offer::from_spend_bundle(
            &mut self.ctx,
            &SpendBundle::aggregate(spend_bundles),
        )?)
    }

    fn take(
        &mut self,
        taker: &BlsPair,
        offer: &Offer,
        add: impl FnOnce(&mut Spends),
    ) -> Result<Outputs> {
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
    ) -> Result<Outputs> {
        let mut spends = Spends::new(taker.puzzle_hash);
        spends.add(offer.offered_coins().clone());
        add(&mut spends);

        let mut actions = Vec::new();

        for &launcher_id in offer.requested_payments().nfts.keys() {
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

    fn royalty(&mut self, id: Id, nft: &Nft, puzzle_hash: Bytes32, amount: u64) -> Result<Action> {
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

    assert_eq!(
        payable_trade_prices(&trade_prices, 300),
        vec![xch_trade_price(34), cat_trade_price(asset_id, 1000)]
    );
    assert_eq!(payable_trade_prices(&trade_prices, 0), Vec::new());
    assert_eq!(payable_trade_prices(&[], 300), Vec::new());
}

#[rstest]
#[case::xch(Price::Xch)]
#[case::cat(Price::Cat)]
#[case::revocable_cat(Price::RevocableCat)]
fn test_sell_nft(#[case] price: fn(u64) -> Price) -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let funds = t.fund_in(&bob, price(1030))?;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &funds.amounts(1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, funds.amounts(30));

    t.take(&bob, &offer, |spends| funds.add_to(spends))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.funds_balance(funds, alice.puzzle_hash), 1000);
    assert_eq!(t.funds_balance(funds, ROYALTY_A), 30);
    assert_eq!(t.funds_balance(funds, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_royalty_rounds_down() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

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
fn test_royalty_must_match_exactly() -> Result<()> {
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
fn test_royalty_must_be_paid_in_trade_price_asset() -> Result<()> {
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
fn test_sell_nft_for_xch_and_cat() -> Result<()> {
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
fn test_each_trade_price_is_paid_separately() -> Result<()> {
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
fn test_identical_royalty_payments_are_paid_once() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

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
fn test_nft_for_nft_has_no_royalties() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;

    let offer = t.swap_nfts(&alice, &alice_nft, &bob_nft)?;
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
fn test_royalty_free_nft() -> Result<()> {
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
fn test_zero_royalty_trade_price_is_unpayable() -> Result<()> {
    for (basis_points, price) in [(300, 10), (0, 1000)] {
        let mut t = Tester::new();
        let alice = t.pair();
        let bob = t.pair();

        let nft = t.mint_nft(&alice, ROYALTY_A, basis_points)?;
        let offer = t.sell_nfts_with(&alice, &[nft], &xch(price), |_, _| {
            vec![xch_trade_price(price)]
        })?;
        let offer = t.aggregate(&[offer])?;

        assert!(matches!(
            offer.requested_royalty_amounts(),
            Err(DriverError::ZeroRoyaltyPayment(launcher_id)) if launcher_id == nft.info.launcher_id
        ));
        assert!(matches!(
            offer.requested_royalty_payments(&mut t.ctx),
            Err(DriverError::ZeroRoyaltyPayment(launcher_id)) if launcher_id == nft.info.launcher_id
        ));

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
fn test_payable_trade_prices_make_small_sales_takeable() -> Result<()> {
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
fn test_unknown_trade_price_asset_is_rejected() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let unknown_asset_id = Bytes32::new([8; 32]);
    let unknown = cat_trade_price(unknown_asset_id, 1000);
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1000), |_, _| vec![unknown])?;
    let offer = t.aggregate(&[offer])?;

    assert!(matches!(
        offer.requested_royalty_amounts(),
        Err(DriverError::UnknownSettlementAsset(puzzle_hash)) if puzzle_hash == unknown.puzzle_hash
    ));
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::UnknownSettlementAsset(puzzle_hash)) if puzzle_hash == unknown.puzzle_hash
    ));

    Ok(())
}

#[test]
fn test_aggregate_sells_same_price_different_royalties() -> Result<()> {
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
fn test_aggregate_sells_different_prices_different_royalties() -> Result<()> {
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
fn test_aggregate_sells_different_prices_same_royalty() -> Result<()> {
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
fn test_aggregate_identical_royalties_for_different_nfts() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

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
fn test_aggregate_multi_nft_offer_with_single_nft_offer() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

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
fn test_aggregate_sells_for_different_assets() -> Result<()> {
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
fn test_aggregate_nft_sale_with_unrelated_cat_trade() -> Result<()> {
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
fn test_aggregate_nft_for_nft_with_nft_sale() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let nft_a = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let nft_b = t.mint_nft(&bob, ROYALTY_B, 300)?;
    let nft_c = t.mint_nft(&carol, ROYALTY_C, 300)?;

    // Alice's NFT commits to no trade price, so only Bob's NFT is owed a royalty
    let alice_offer = t.swap_nfts(&alice, &nft_a, &nft_c)?;
    let bob_offer = t.sell_nfts(&bob, &[nft_b], &xch(1000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(30));
    assert_eq!(offer.offered_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(
        offer.requested_nft_trade_prices(nft_c.info.launcher_id),
        Vec::new()
    );

    let coin = t.fund(&carol, 1030);
    t.take(&carol, &offer, |spends| {
        spends.add(coin);
        spends.add(nft_c);
    })?;

    assert!(t.owns_nft(&nft_a, carol.puzzle_hash));
    assert!(t.owns_nft(&nft_b, carol.puzzle_hash));
    assert!(t.owns_nft(&nft_c, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1000);
    assert_eq!(t.balance(ROYALTY_A), 0);
    assert_eq!(t.balance(ROYALTY_B), 30);
    assert_eq!(t.balance(ROYALTY_C), 0);

    Ok(())
}

#[test]
fn test_bid_for_nft() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::Xch(1999))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(59));
    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1999)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1999);
    assert_eq!(t.balance(ROYALTY_A), 59);

    Ok(())
}

#[rstest]
#[case::cat(Price::Cat, cat_trade_price)]
#[case::revocable_cat(Price::RevocableCat, revocable_cat_trade_price)]
fn test_bid_for_nft_with_cat(
    #[case] price: fn(u64) -> Price,
    #[case] trade_price: fn(Bytes32, u64) -> TradePrice,
) -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, price(1000))?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(offer.offered_royalty_amounts()?, cat(asset_id, 30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![trade_price(asset_id, 1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_bid_for_nft_in_two_assets() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

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
fn test_aggregate_bids_same_price_different_royalties() -> Result<()> {
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
fn test_aggregate_bids_different_prices() -> Result<()> {
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
fn test_extend_bids() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(30_000))?;

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
fn test_unmatchable_prepaid_royalty_is_not_revealed() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

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
fn test_prepaid_royalties_for_royalty_free_nft_are_not_revealed() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

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
fn test_smallest_trade_price_for_prepaid_royalty() -> Result<()> {
    let launcher_id = Bytes32::new([4; 32]);

    let trade_prices = |basis_points: u16, royalty: u64| -> Result<Vec<TradePrice>> {
        let mut ctx = SpendContext::new();
        let mut offered_coins = OfferCoins::new();
        offered_coins.prepaid_royalties.xch = vec![
            RoyaltyInfo::new(launcher_id, ROYALTY_A, basis_points).payment(&mut ctx, royalty)?,
        ];

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

    assert_eq!(trade_prices(300, 30)?, vec![xch_trade_price(1000)]);
    assert_eq!(trade_prices(300, 31)?, vec![xch_trade_price(1034)]);
    assert_eq!(trade_prices(10_000, 7)?, vec![xch_trade_price(7)]);
    assert_eq!(trade_prices(20_000, 8)?, vec![xch_trade_price(4)]);

    assert_eq!(trade_prices(20_000, 7)?, Vec::new());
    assert_eq!(trade_prices(1, u64::MAX)?, Vec::new());

    for basis_points in [1, 3, 250, 300, 9999, 10_000, 10_001, 65_535] {
        for royalty in (1..=200).chain([u64::MAX / 7, u64::MAX]) {
            let trade_prices = trade_prices(basis_points, royalty)?;

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
                    amount == 0
                        || calculate_nft_royalty(amount - 1, basis_points) < u128::from(royalty)
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_aggregate_sale_and_bid_for_same_nft() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let bob_offer = t.bid(&bob, &nft, Price::Xch(1500))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(50));
    assert_eq!(offer.offered_royalty_amounts()?, xch(75));

    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&carol, &offer, |_| {})?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 450);
    assert_eq!(t.balance(ROYALTY_A), 50 + 75);

    Ok(())
}

#[test]
fn test_prepaid_royalty_is_not_paid_again() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Bob's bid prepays the same royalty that Alice's NFT asserts, which satisfies it
    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let bob_offer = t.bid(&bob, &nft, Price::Xch(1000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(offer.offered_royalty_amounts()?, xch(50));

    t.take(&carol, &offer, |_| {})?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);
    assert_eq!(t.balance(ROYALTY_A), 50);

    Ok(())
}

#[test]
fn test_aggregate_bids_reveal_split_trade_price() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Each bid is 1999, which the even split of the combined 3998 reproduces, so each NFT reveals
    // it rather than the smallest trade price with the same royalty, 1967
    let first_nft = t.mint_nft(&carol, ROYALTY_A, 300)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 300)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(1999))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(1999))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    for nft in [first_nft, second_nft] {
        assert_eq!(
            offer.requested_nft_trade_prices(nft.info.launcher_id),
            vec![xch_trade_price(1999)]
        );
    }

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert_eq!(t.balance(carol.puzzle_hash), 3998);
    assert_eq!(t.balance(ROYALTY_A), 59);
    assert_eq!(t.balance(ROYALTY_B), 59);

    Ok(())
}

#[test]
fn test_malformed_prepaid_royalties_are_ignored() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let launcher_id = nft.info.launcher_id;
    let royalty_hint = t.ctx.hint(ROYALTY_A)?;
    let other_hint = t.ctx.hint(ROYALTY_B)?;

    let malformed = vec![
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, 30, Memos::None)]),
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, 30, other_hint)]),
        NotarizedPayment::new(
            launcher_id,
            vec![
                Payment::new(ROYALTY_A, 30, royalty_hint),
                Payment::new(ROYALTY_B, 1, other_hint),
            ],
        ),
    ];

    let offer = t.bid_with_settlements(&alice, &nft, &[(Price::Xch(1000), malformed.clone())])?;
    let offer = t.aggregate(&[offer])?;

    assert!(offer.offered_coins().prepaid_royalties.xch.is_empty());
    assert_eq!(offer.offered_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 90);

    let nft = t.mint_nft(&bob, ROYALTY_B, 300)?;
    let mut settlements = vec![NotarizedPayment::new(
        nft.info.launcher_id,
        vec![Payment::new(ROYALTY_B, 30, Memos::None)],
    )];
    settlements
        .push(RoyaltyInfo::new(nft.info.launcher_id, ROYALTY_B, 300).payment(&mut t.ctx, 30)?);

    let offer = t.bid_with_settlements(&alice, &nft, &[(Price::Xch(1000), settlements)])?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_B), 1 + 30 + 30);

    Ok(())
}

#[test]
fn test_bid_for_nft_and_xch() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;

    // Alice offers 1000 for the NFT plus 100 back, prepaying the royalty on her gross 1000. The
    // NFT has to reveal that gross trade price, not the net 900.
    let royalty = coin_amount(calculate_nft_royalty(1000, nft.info.royalty_basis_points))?;
    let settlement = RoyaltyInfo::new(
        nft.info.launcher_id,
        nft.info.royalty_puzzle_hash,
        nft.info.royalty_basis_points,
    )
    .payment(&mut t.ctx, royalty)?;
    let offer = t.bid_with_settlements_and_request(
        &alice,
        &nft,
        &[(Price::Xch(1000), vec![settlement])],
        &xch(100),
    )?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1000)]
    );

    let coin = t.fund(&bob, 100);
    t.take(&bob, &offer, |spends| {
        spends.add(coin);
        spends.add(nft);
    })?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 100);
    assert_eq!(t.balance(bob.puzzle_hash), 1000);
    assert_eq!(t.balance(ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_prepaid_royalties_from_untrusted_offer() -> Result<()> {
    let mut ctx = SpendContext::new();
    let launcher_id = Bytes32::new([4; 32]);
    let royalty = RoyaltyInfo::new(launcher_id, ROYALTY_A, 1);

    let mut offered_coins = OfferCoins::new();
    offered_coins.prepaid_royalties.xch = vec![
        royalty.payment(&mut ctx, u64::MAX)?,
        royalty.payment(&mut ctx, u64::MAX)?,
        RoyaltyInfo::new(launcher_id, ROYALTY_B, 1).payment(&mut ctx, 1000)?,
        RoyaltyInfo::new(Bytes32::new([5; 32]), ROYALTY_A, 1).payment(&mut ctx, 1000)?,
    ];

    let mut requested_payments = RequestedPayments::new();
    requested_payments.nfts.insert(
        launcher_id,
        vec![NotarizedPayment::new(
            Bytes32::default(),
            vec![Payment::new(ROYALTY_C, 1, Memos::None)],
        )],
    );

    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        launcher_id,
        NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, 1),
    )?;

    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins.clone(),
        requested_payments.clone(),
        asset_info.clone(),
    );

    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 2 * u128::from(u64::MAX),
            cats: IndexMap::new(),
        }
    );

    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());

    let unknown_launcher_id = Bytes32::new([6; 32]);
    requested_payments
        .nfts
        .insert(unknown_launcher_id, Vec::new());
    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins,
        requested_payments,
        asset_info,
    );

    assert!(matches!(
        offer.offered_royalty_amounts(),
        Err(DriverError::MissingAssetInfo)
    ));
    assert_eq!(
        offer.requested_nft_trade_prices(unknown_launcher_id),
        Vec::new()
    );

    Ok(())
}

#[test]
fn test_unparseable_prepaid_royalties_are_ignored() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let settlement = t.ctx.alloc_mod::<SettlementPayment>()?;

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

    assert!(offer.offered_coins().prepaid_royalties.xch.is_empty());
    assert!(offer.offered_coins().prepaid_royalties.cats.is_empty());

    Ok(())
}

#[test]
fn test_invalid_nft_spends_are_rejected() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let raise = t.ctx.alloc(&clvm_list!(8))?;

    let spend_nft = |ctx: &mut SpendContext, inner_puzzle| -> Result<()> {
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
fn test_offered_nft_without_known_trade_prices() -> Result<()> {
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
fn test_royalty_larger_than_a_coin_is_rejected() -> Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, u16::MAX)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1), |_, _| {
        vec![xch_trade_price(u64::MAX)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert!(matches!(
        offer.requested_royalty_amounts(),
        Err(DriverError::AmountOverflow)
    ));
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::AmountOverflow)
    ));

    Ok(())
}

#[test]
fn test_missing_asset_info_is_an_error() {
    let mut ctx = SpendContext::new();

    let launcher_id = Bytes32::new([1; 32]);
    let asset_id = Bytes32::new([2; 32]);
    let notarized_payment = NotarizedPayment::new(
        Bytes32::default(),
        vec![Payment::new(Bytes32::default(), 1, Memos::None)],
    );

    let mut requested_payments = RequestedPayments::new();
    requested_payments
        .nfts
        .insert(launcher_id, vec![notarized_payment.clone()]);

    let offer = Offer::new(
        SpendBundle::new(vec![], Signature::default()),
        OfferCoins::new(),
        requested_payments,
        AssetInfo::new(),
    );
    assert!(matches!(
        offer.offered_royalties(),
        Err(DriverError::MissingAssetInfo)
    ));

    let mut requested_payments = RequestedPayments::new();
    requested_payments
        .cats
        .insert(asset_id, vec![notarized_payment]);
    assert!(matches!(
        requested_payments.assertions(&mut ctx, &AssetInfo::new()),
        Err(DriverError::MissingAssetInfo)
    ));

    let amounts = OfferAmounts {
        xch: 0,
        cats: indexmap! { asset_id => 100 },
    };
    assert!(matches!(
        calculate_trade_prices(&amounts, &AssetInfo::new()),
        Err(DriverError::MissingAssetInfo)
    ));
}
