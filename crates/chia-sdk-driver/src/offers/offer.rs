use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::TradePrice, puzzles::SettlementPayment, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::Allocator;
use indexmap::IndexSet;

use crate::{
    Arbitrage, AssetInfo, CatInfo, DriverError, Id, Layer, NftInfo, OfferAmounts, OfferCoins,
    OptionInfo, Puzzle, RequestedPayments, RoyaltyInfo, SingletonInfo, SpendContext,
    calculate_nft_royalty, coin_amount,
};

#[derive(Debug, Clone)]
pub struct Offer {
    spend_bundle: SpendBundle,
    offered_coins: OfferCoins,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
}

impl Offer {
    pub fn new(
        spend_bundle: SpendBundle,
        offered_coins: OfferCoins,
        requested_payments: RequestedPayments,
        asset_info: AssetInfo,
    ) -> Self {
        Self {
            spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        }
    }

    pub fn cancellable_coin_spends(&self) -> Result<Vec<CoinSpend>, DriverError> {
        let mut allocator = Allocator::new();
        let mut created_coin_ids = HashSet::new();

        for coin_spend in &self.spend_bundle.coin_spends {
            let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
            let solution = coin_spend.solution.to_clvm(&mut allocator)?;

            let output = run_puzzle(&mut allocator, puzzle, solution)?;
            let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

            for condition in conditions {
                if let Some(create_coin) = condition.into_create_coin() {
                    created_coin_ids.insert(
                        Coin::new(
                            coin_spend.coin.coin_id(),
                            create_coin.puzzle_hash,
                            create_coin.amount,
                        )
                        .coin_id(),
                    );
                }
            }
        }

        Ok(self
            .spend_bundle
            .coin_spends
            .iter()
            .filter_map(|cs| {
                if created_coin_ids.contains(&cs.coin.coin_id()) {
                    None
                } else {
                    Some(cs.clone())
                }
            })
            .collect())
    }

    pub fn spend_bundle(&self) -> &SpendBundle {
        &self.spend_bundle
    }

    pub fn offered_coins(&self) -> &OfferCoins {
        &self.offered_coins
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    /// Returns the royalty info for requested NFTs, since those are the royalties
    /// that need to be paid by the offered side.
    pub fn offered_royalties(&self) -> Vec<RoyaltyInfo> {
        self.requested_payments
            .nfts
            .keys()
            .filter_map(|&launcher_id| {
                self.asset_info.nft(launcher_id).map(|nft| {
                    RoyaltyInfo::new(
                        launcher_id,
                        nft.royalty_puzzle_hash,
                        nft.royalty_basis_points,
                    )
                })
            })
            .filter(|royalty| royalty.basis_points > 0)
            .collect()
    }

    /// Returns the royalty info for offered NFTs, since those are the royalties
    /// that need to be paid by the requested side.
    pub fn requested_royalties(&self) -> Vec<RoyaltyInfo> {
        self.offered_coins
            .nfts
            .values()
            .map(|nft| {
                RoyaltyInfo::new(
                    nft.info.launcher_id,
                    nft.info.royalty_puzzle_hash,
                    nft.info.royalty_basis_points,
                )
            })
            .filter(|royalty| royalty.basis_points > 0)
            .collect()
    }

    /// Returns the royalties already paid in the offer for requested NFTs.
    pub fn offered_royalty_amounts(&self) -> Result<OfferAmounts, DriverError> {
        let mut amounts = OfferAmounts::new();

        for (&launcher_id, nft) in self
            .requested_payments
            .nfts
            .keys()
            .filter_map(|launcher_id| Some((launcher_id, self.asset_info.nft(*launcher_id)?)))
        {
            for (asset, amount) in self.prepaid_royalties(launcher_id, nft.royalty_puzzle_hash) {
                add_amount(&mut amounts, asset, amount.into());
            }
        }

        Ok(amounts)
    }

    /// Returns the royalties that need to be paid for offered NFTs.
    ///
    /// Trade prices whose royalty can't be paid are left out, see [`Offer::requested_royalty_payments`].
    pub fn requested_royalty_amounts(&self) -> Result<OfferAmounts, DriverError> {
        let mut amounts = OfferAmounts::new();

        for (_, settlement_puzzle_hash, amount) in self.committed_royalties() {
            if amount > 0
                && let Some(asset) = self.trade_price_asset(settlement_puzzle_hash)
            {
                add_amount(&mut amounts, asset, amount);
            }
        }

        Ok(amounts)
    }

    /// Returns the royalty payments that need to be settled for offered NFTs.
    ///
    /// Each offered NFT asserts a royalty payment for every trade price it revealed when it was
    /// spent into settlement. Since those trade prices are chosen by each maker, they can't be
    /// derived from the totals of an aggregated offer. Identical payments are only included once,
    /// since a single announcement satisfies all of the NFT's assertions for it.
    ///
    /// Settlement payments must be positive, so this fails with
    /// [`DriverError::ZeroRoyaltyPayment`] if a trade price's royalty rounds down to zero. It fails
    /// with [`DriverError::AmountOverflow`] if a royalty doesn't fit in a [`u64`].
    pub fn requested_royalty_payments(
        &self,
        ctx: &mut SpendContext,
    ) -> Result<RequestedPayments, DriverError> {
        let mut payments = RequestedPayments::new();

        for (royalty, settlement_puzzle_hash, amount) in self.committed_royalties() {
            let asset = self
                .trade_price_asset(settlement_puzzle_hash)
                .ok_or(DriverError::UnknownTradePriceAsset(settlement_puzzle_hash))?;

            if amount == 0 {
                return Err(DriverError::ZeroRoyaltyPayment(royalty.launcher_id));
            }

            let payment = royalty.payment(ctx, coin_amount(amount)?)?;

            if let Id::Existing(asset_id) = asset {
                payments.cats.entry(asset_id).or_default().push(payment);
            } else {
                payments.xch.push(payment);
            }
        }

        Ok(payments)
    }

    /// Returns the trade prices a requested NFT should reveal when the taker spends it into
    /// settlement, so that it asserts exactly the royalties the offer already paid for it.
    ///
    /// Prepaid royalties that no trade price could produce are left out, since the NFT can't
    /// assert them.
    pub fn requested_nft_trade_prices(&self, launcher_id: Bytes32) -> Vec<TradePrice> {
        let Some(nft) = self.asset_info.nft(launcher_id) else {
            return Vec::new();
        };

        let basis_points = nft.royalty_basis_points;

        if basis_points == 0 {
            return Vec::new();
        }

        self.prepaid_royalties(launcher_id, nft.royalty_puzzle_hash)
            .into_iter()
            .filter_map(|(asset, royalty)| {
                let amount = u64::try_from(
                    (u128::from(royalty) * 10_000).div_ceil(u128::from(basis_points)),
                )
                .ok()?;

                (calculate_nft_royalty(amount, basis_points) == u128::from(royalty))
                    .then(|| TradePrice::new(amount, self.settlement_puzzle_hash(asset)))
            })
            .collect()
    }

    /// Each distinct royalty asserted by offered NFTs, as the NFT's royalty info along with the
    /// settlement puzzle hash and amount of the payment.
    fn committed_royalties(&self) -> IndexSet<(RoyaltyInfo, Bytes32, u128)> {
        self.offered_coins
            .nfts
            .iter()
            .flat_map(|(&launcher_id, nft)| {
                let royalty = RoyaltyInfo::new(
                    launcher_id,
                    nft.info.royalty_puzzle_hash,
                    nft.info.royalty_basis_points,
                );

                self.offered_coins
                    .nft_trade_prices
                    .get(&launcher_id)
                    .into_iter()
                    .flatten()
                    .map(move |trade_price| {
                        (
                            royalty,
                            trade_price.puzzle_hash,
                            calculate_nft_royalty(trade_price.amount, royalty.basis_points),
                        )
                    })
            })
            .collect()
    }

    /// Royalty payments for an NFT made by settlement spends in the offer, along with the asset
    /// they're paid in.
    fn prepaid_royalties(
        &self,
        launcher_id: Bytes32,
        royalty_puzzle_hash: Bytes32,
    ) -> Vec<(Id, u64)> {
        let settled = &self.offered_coins.settled_payments;

        settled
            .xch
            .iter()
            .map(|notarized_payment| (Id::Xch, notarized_payment))
            .chain(
                settled
                    .cats
                    .iter()
                    .flat_map(|(&asset_id, notarized_payments)| {
                        notarized_payments.iter().map(move |notarized_payment| {
                            (Id::Existing(asset_id), notarized_payment)
                        })
                    }),
            )
            .filter(|(_, notarized_payment)| notarized_payment.nonce == launcher_id)
            .flat_map(|(asset, notarized_payment)| {
                notarized_payment
                    .payments
                    .iter()
                    .filter(|payment| payment.puzzle_hash == royalty_puzzle_hash)
                    .map(move |payment| (asset, payment.amount))
            })
            .collect()
    }

    /// The asset paid to a trade price's settlement puzzle hash, if it's known.
    fn trade_price_asset(&self, settlement_puzzle_hash: Bytes32) -> Option<Id> {
        if settlement_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
            return Some(Id::Xch);
        }

        self.asset_info
            .cats()
            .find(|&&asset_id| {
                self.settlement_puzzle_hash(Id::Existing(asset_id)) == settlement_puzzle_hash
            })
            .map(|&asset_id| Id::Existing(asset_id))
    }

    fn settlement_puzzle_hash(&self, asset: Id) -> Bytes32 {
        let Id::Existing(asset_id) = asset else {
            return SETTLEMENT_PAYMENT_HASH.into();
        };

        let hidden_puzzle_hash = self
            .asset_info
            .cat(asset_id)
            .and_then(|info| info.hidden_puzzle_hash);

        CatInfo::new(asset_id, hidden_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into())
            .puzzle_hash()
            .into()
    }

    pub fn arbitrage(&self) -> Arbitrage {
        let offered = self.offered_coins.amounts();
        let requested = self.requested_payments.amounts();

        let mut arbitrage = Arbitrage::new();

        if requested.xch > offered.xch {
            arbitrage.offered.xch = requested.xch - offered.xch;
        } else {
            arbitrage.requested.xch = offered.xch - requested.xch;
        }

        for &asset_id in offered
            .cats
            .keys()
            .chain(requested.cats.keys())
            .collect::<IndexSet<_>>()
        {
            let &offered_amount = offered.cats.get(&asset_id).unwrap_or(&0);
            let &requested_amount = requested.cats.get(&asset_id).unwrap_or(&0);

            if requested_amount > offered_amount {
                let diff = requested_amount - offered_amount;
                arbitrage.offered.cats.insert(asset_id, diff);
            } else {
                let diff = offered_amount - requested_amount;
                arbitrage.requested.cats.insert(asset_id, diff);
            }
        }

        for &launcher_id in self
            .offered_coins
            .nfts
            .keys()
            .chain(self.requested_payments.nfts.keys())
            .collect::<IndexSet<_>>()
        {
            let is_offered = self.offered_coins.nfts.contains_key(&launcher_id);
            let is_requested = self.requested_payments.nfts.contains_key(&launcher_id);

            if is_offered && !is_requested {
                arbitrage.requested.nfts.push(launcher_id);
            } else if !is_offered && is_requested {
                arbitrage.offered.nfts.push(launcher_id);
            }
        }

        for &launcher_id in self
            .offered_coins
            .options
            .keys()
            .chain(self.requested_payments.options.keys())
            .collect::<IndexSet<_>>()
        {
            let is_offered = self.offered_coins.options.contains_key(&launcher_id);
            let is_requested = self.requested_payments.options.contains_key(&launcher_id);

            if is_offered && !is_requested {
                arbitrage.requested.options.push(launcher_id);
            } else if !is_offered && is_requested {
                arbitrage.offered.options.push(launcher_id);
            }
        }

        arbitrage
    }

    pub fn nonce(mut coin_ids: Vec<Bytes32>) -> Bytes32 {
        coin_ids.sort();
        coin_ids.tree_hash().into()
    }

    pub fn from_input_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: SpendBundle,
        requested_payments: RequestedPayments,
        requested_asset_info: AssetInfo,
    ) -> Result<Self, DriverError> {
        let mut offered_coins = OfferCoins::new();
        let mut asset_info = requested_asset_info;

        let spent_coin_ids: HashSet<Bytes32> = spend_bundle
            .coin_spends
            .iter()
            .map(|cs| cs.coin.coin_id())
            .collect();

        for coin_spend in &spend_bundle.coin_spends {
            let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
            let puzzle = Puzzle::parse(allocator, puzzle);
            let solution = coin_spend.solution.to_clvm(allocator)?;

            offered_coins.parse(
                allocator,
                &mut asset_info,
                &spent_coin_ids,
                coin_spend.coin,
                puzzle,
                solution,
            )?;
        }

        Ok(Self::new(
            spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        ))
    }

    pub fn from_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: &SpendBundle,
    ) -> Result<Self, DriverError> {
        let mut input_spend_bundle =
            SpendBundle::new(Vec::new(), spend_bundle.aggregated_signature.clone());
        let mut offered_coins = OfferCoins::new();
        let mut requested_payments = RequestedPayments::new();
        let mut asset_info = AssetInfo::new();

        let spent_coin_ids: HashSet<Bytes32> = spend_bundle
            .coin_spends
            .iter()
            .filter_map(|cs| {
                if cs.coin.parent_coin_info == Bytes32::default() {
                    None
                } else {
                    Some(cs.coin.coin_id())
                }
            })
            .collect();

        for coin_spend in &spend_bundle.coin_spends {
            let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
            let puzzle = Puzzle::parse(allocator, puzzle);
            let solution = coin_spend.solution.to_clvm(allocator)?;

            if coin_spend.coin.parent_coin_info == Bytes32::default() {
                requested_payments.parse(allocator, &mut asset_info, puzzle, solution)?;
            } else {
                input_spend_bundle.coin_spends.push(coin_spend.clone());

                offered_coins.parse(
                    allocator,
                    &mut asset_info,
                    &spent_coin_ids,
                    coin_spend.coin,
                    puzzle,
                    solution,
                )?;
            }
        }

        Ok(Self::new(
            input_spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        ))
    }

    pub fn to_spend_bundle(mut self, ctx: &mut SpendContext) -> Result<SpendBundle, DriverError> {
        let settlement = ctx.alloc_mod::<SettlementPayment>()?;

        if !self.requested_payments.xch.is_empty() {
            let solution = SettlementPaymentsSolution::new(self.requested_payments.xch);

            self.spend_bundle.coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), SETTLEMENT_PAYMENT_HASH.into(), 0),
                ctx.serialize(&settlement)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (asset_id, notarized_payments) in self.requested_payments.cats {
            let cat_info = CatInfo::new(
                asset_id,
                self.asset_info
                    .cat(asset_id)
                    .and_then(|info| info.hidden_puzzle_hash),
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = cat_info.construct_puzzle(ctx, settlement)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            self.spend_bundle.coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), cat_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (launcher_id, notarized_payments) in self.requested_payments.nfts {
            let info = self
                .asset_info
                .nft(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let nft_info = NftInfo::new(
                launcher_id,
                info.metadata,
                info.metadata_updater_puzzle_hash,
                None,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = nft_info.into_layers(settlement).construct_puzzle(ctx)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            self.spend_bundle.coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), nft_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (launcher_id, notarized_payments) in self.requested_payments.options {
            let info = self
                .asset_info
                .option(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let option_info = OptionInfo::new(
                launcher_id,
                info.underlying_coin_id,
                info.underlying_delegated_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = option_info.into_layers(settlement).construct_puzzle(ctx)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            self.spend_bundle.coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), option_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        Ok(self.spend_bundle)
    }

    pub fn extend(&mut self, other: Self) -> Result<(), DriverError> {
        self.spend_bundle
            .coin_spends
            .extend(other.spend_bundle.coin_spends);
        self.spend_bundle.aggregated_signature += &other.spend_bundle.aggregated_signature;
        self.offered_coins.extend(other.offered_coins)?;
        self.requested_payments.extend(other.requested_payments);
        self.asset_info.extend(other.asset_info)?;

        Ok(())
    }

    pub fn take(self, spend_bundle: SpendBundle) -> SpendBundle {
        SpendBundle::new(
            [self.spend_bundle.coin_spends, spend_bundle.coin_spends].concat(),
            self.spend_bundle.aggregated_signature + &spend_bundle.aggregated_signature,
        )
    }
}

fn add_amount(amounts: &mut OfferAmounts, asset: Id, amount: u128) {
    let total = if let Id::Existing(asset_id) = asset {
        amounts.cats.entry(asset_id).or_default()
    } else {
        &mut amounts.xch
    };

    *total += amount;
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_puzzle_types::{
        Memos,
        offer::{NotarizedPayment, Payment},
    };
    use chia_sdk_test::{Simulator, sign_transaction};
    use indexmap::indexmap;

    use crate::{Action, NftAssetInfo, Relation, SpendContext, Spends};

    use super::*;

    #[test]
    fn test_offer_nft_for_nft() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = sim.bls(0);

        let alice_hint = ctx.hint(alice.puzzle_hash)?;
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        // Mint NFTs
        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::mint_empty_royalty_nft(alice.puzzle_hash, 300),
                Action::mint_empty_royalty_nft(bob.puzzle_hash, 300),
                Action::send(Id::New(1), bob.puzzle_hash, 1, bob_hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let alice_nft = outputs.nfts[&Id::New(0)];
        let bob_nft = outputs.nfts[&Id::New(1)];

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Make offer
        let mut requested_payments = RequestedPayments::new();
        let mut requested_asset_info = AssetInfo::new();

        requested_payments.nfts.insert(
            bob_nft.info.launcher_id,
            vec![NotarizedPayment::new(
                Offer::nonce(vec![alice_nft.coin.coin_id()]),
                vec![Payment::new(alice.puzzle_hash, 1, alice_hint)],
            )],
        );
        requested_asset_info.insert_nft(
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

        let deltas = spends.apply(
            &mut ctx,
            &[Action::send(
                Id::Existing(alice_nft.info.launcher_id),
                SETTLEMENT_PAYMENT_HASH.into(),
                1,
                Memos::None,
            )],
        )?;

        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut ctx, &requested_asset_info)?);

        spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[alice.sk])?;

        let offer = Offer::from_input_spend_bundle(
            &mut ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            requested_asset_info,
        )?;

        // Take offer
        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(offer.offered_coins().clone());
        spends.add(bob_nft);

        let deltas = spends.apply(&mut ctx, &offer.requested_payments().actions())?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[bob.sk])?;

        let spend_bundle = offer.take(SpendBundle::new(coin_spends, signature));

        sim.new_transaction(spend_bundle)?;

        let final_bob_nft = outputs.nfts[&Id::Existing(alice_nft.info.launcher_id)];
        let final_alice_nft = outputs.nfts[&Id::Existing(bob_nft.info.launcher_id)];

        assert_eq!(final_bob_nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(final_alice_nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }
}
