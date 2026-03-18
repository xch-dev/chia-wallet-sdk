use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, puzzles::SettlementPayment, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::Allocator;
use indexmap::IndexSet;

use crate::{
    Action, Arbitrage, AssetInfo, CatInfo, CatTransferFeeContext, DriverError, Id, Layer, NftInfo, OfferAmounts,
    OfferCoins, OptionInfo, Puzzle, RequestedPayments, RoyaltyInfo, SingletonInfo, SpendContext,
    Spends, TransferFeeInfo, calculate_royalty_amounts, calculate_trade_price_amounts,
    calculate_trade_prices, calculate_transfer_fee_amounts, calculate_transfer_fee_payments,
    ensure_trade_prices_supported,
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

    /// Returns transfer-fee policies for requested CATs (fees paid by the offered side).
    pub fn offered_transfer_fees(&self) -> Vec<TransferFeeInfo> {
        self.requested_payments
            .cats
            .keys()
            .filter_map(|&asset_id| {
                let policy = self
                    .asset_info
                    .cat(asset_id)
                    .and_then(|cat| cat.transfer_fee_policy.clone())?;
                Some(TransferFeeInfo::new(asset_id, policy))
            })
            .filter(|info| info.policy.fee_basis_points > 0)
            .collect()
    }

    /// Returns requested CAT ids that require CAT trade context.
    fn offered_fee_context_asset_ids(&self) -> Vec<Bytes32> {
        self.requested_payments
            .cats
            .keys()
            .copied()
            .filter(|&asset_id| {
                self.asset_info
                    .cat(asset_id)
                    .and_then(|cat| cat.transfer_fee_policy.clone())
                    .is_some()
            })
            .collect()
    }

    /// Returns transfer-fee policies for offered CATs (fees paid by the requested side).
    pub fn requested_transfer_fees(&self) -> Vec<TransferFeeInfo> {
        let mut infos = Vec::new();
        let mut seen = IndexSet::new();

        for cats in self.offered_coins.cats.values() {
            for cat in cats {
                let Some(policy) = cat.info.transfer_fee_policy.clone() else {
                    continue;
                };

                if seen.insert(cat.info.asset_id) {
                    infos.push(TransferFeeInfo::new(cat.info.asset_id, policy));
                }
            }
        }

        infos
            .into_iter()
            .filter(|info| info.policy.fee_basis_points > 0)
            .collect()
    }

    /// Returns offered CAT ids that require CAT trade context.
    fn requested_fee_context_asset_ids(&self) -> Vec<Bytes32> {
        let mut ids = IndexSet::new();

        for cats in self.offered_coins.cats.values() {
            for cat in cats {
                if cat.info.transfer_fee_policy.is_some() {
                    ids.insert(cat.info.asset_id);
                }
            }
        }

        ids.into_iter().collect()
    }

    pub fn offered_transfer_fee_amounts(&self) -> OfferAmounts {
        let offered_amounts = self.offered_coins.amounts();
        let transfer_fees = self.offered_transfer_fees();
        calculate_transfer_fee_amounts(&offered_amounts, &transfer_fees)
    }

    pub fn requested_transfer_fee_amounts(&self) -> OfferAmounts {
        let requested_amounts = self.requested_payments.amounts();
        let transfer_fees = self.requested_transfer_fees();
        calculate_transfer_fee_amounts(&requested_amounts, &transfer_fees)
    }

    pub fn offered_transfer_fee_payments(
        &self,
        ctx: &mut SpendContext,
        trade_nonce: Bytes32,
    ) -> Result<RequestedPayments, DriverError> {
        let transfer_fees = self.offered_transfer_fees();
        if transfer_fees.is_empty() {
            return Ok(RequestedPayments::new());
        }
        let offered_amounts = self.offered_coins.amounts();
        let trade_prices = calculate_trade_prices(&offered_amounts, &self.asset_info);
        ensure_trade_prices_supported(&trade_prices)?;
        calculate_transfer_fee_payments(ctx, trade_nonce, &offered_amounts, &transfer_fees)
    }

    pub fn requested_transfer_fee_payments(
        &self,
        ctx: &mut SpendContext,
        trade_nonce: Bytes32,
    ) -> Result<RequestedPayments, DriverError> {
        let transfer_fees = self.requested_transfer_fees();
        if transfer_fees.is_empty() {
            return Ok(RequestedPayments::new());
        }
        let requested_amounts = self.requested_payments.amounts();
        let trade_prices = calculate_trade_prices(&requested_amounts, &self.asset_info);
        ensure_trade_prices_supported(&trade_prices)?;
        calculate_transfer_fee_payments(ctx, trade_nonce, &requested_amounts, &transfer_fees)
    }

    pub fn offered_royalty_amounts(&self) -> OfferAmounts {
        let offered_amounts = self.offered_coins.amounts();
        let royalties = self.offered_royalties();
        let trade_prices = calculate_trade_price_amounts(&offered_amounts, royalties.len());
        calculate_royalty_amounts(&trade_prices, &royalties)
    }

    pub fn requested_royalty_amounts(&self) -> OfferAmounts {
        let requested_amounts = self.offered_coins.amounts();
        let royalties = self.requested_royalties();
        let trade_prices = calculate_trade_price_amounts(&requested_amounts, royalties.len());
        calculate_royalty_amounts(&trade_prices, &royalties)
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

    pub fn trade_nonce(&self) -> Result<Bytes32, DriverError> {
        let mut trade_nonce = None;

        for notarized_payment in &self.requested_payments.xch {
            if let Some(existing) = trade_nonce {
                if existing != notarized_payment.nonce {
                    return Err(DriverError::InvalidTradeContext);
                }
            } else {
                trade_nonce = Some(notarized_payment.nonce);
            }
        }

        for notarized_payments in self.requested_payments.cats.values() {
            for notarized_payment in notarized_payments {
                if let Some(existing) = trade_nonce {
                    if existing != notarized_payment.nonce {
                        return Err(DriverError::InvalidTradeContext);
                    }
                } else {
                    trade_nonce = Some(notarized_payment.nonce);
                }
            }
        }

        for notarized_payments in self.requested_payments.nfts.values() {
            for notarized_payment in notarized_payments {
                if let Some(existing) = trade_nonce {
                    if existing != notarized_payment.nonce {
                        return Err(DriverError::InvalidTradeContext);
                    }
                } else {
                    trade_nonce = Some(notarized_payment.nonce);
                }
            }
        }

        for notarized_payments in self.requested_payments.options.values() {
            for notarized_payment in notarized_payments {
                if let Some(existing) = trade_nonce {
                    if existing != notarized_payment.nonce {
                        return Err(DriverError::InvalidTradeContext);
                    }
                } else {
                    trade_nonce = Some(notarized_payment.nonce);
                }
            }
        }

        trade_nonce.ok_or(DriverError::MissingTradeContext)
    }

    pub fn take_actions_with_transfer_fees(
        &self,
        ctx: &mut SpendContext,
    ) -> Result<Vec<Action>, DriverError> {
        let mut actions = self.requested_payments.actions();
        let offered_transfer_fees = self.offered_transfer_fees();
        let requested_transfer_fees = self.requested_transfer_fees();

        if offered_transfer_fees.is_empty() && requested_transfer_fees.is_empty() {
            return Ok(actions);
        }

        let trade_nonce = self.trade_nonce()?;
        let offered_fee_payments = self.offered_transfer_fee_payments(ctx, trade_nonce)?;
        let requested_fee_payments = self.requested_transfer_fee_payments(ctx, trade_nonce)?;
        actions.extend(offered_fee_payments.actions());
        actions.extend(requested_fee_payments.actions());
        Ok(actions)
    }

    fn transfer_fee_trade_contexts(&self) -> Result<Vec<(Id, Bytes32, Vec<chia_sdk_types::puzzles::TransferFeeTradePrice>)>, DriverError> {
        let offered_context_asset_ids = self.offered_fee_context_asset_ids();
        let requested_context_asset_ids = self.requested_fee_context_asset_ids();

        if offered_context_asset_ids.is_empty() && requested_context_asset_ids.is_empty() {
            return Ok(Vec::new());
        }

        let trade_nonce = self.trade_nonce()?;
        let offered_amounts = self.offered_coins.amounts();
        let requested_amounts = self.requested_payments.amounts();
        let mut contexts = Vec::new();

        if !offered_context_asset_ids.is_empty() {
            let trade_prices = calculate_trade_prices(&offered_amounts, &self.asset_info);
            ensure_trade_prices_supported(&trade_prices)?;
            for asset_id in offered_context_asset_ids {
                contexts.push((Id::Existing(asset_id), trade_nonce, trade_prices.clone()));
            }
        }

        if !requested_context_asset_ids.is_empty() {
            let trade_prices = calculate_trade_prices(&requested_amounts, &self.asset_info);
            ensure_trade_prices_supported(&trade_prices)?;
            for asset_id in requested_context_asset_ids {
                contexts.push((Id::Existing(asset_id), trade_nonce, trade_prices.clone()));
            }
        }

        Ok(contexts)
    }

    pub fn apply_transfer_fee_trade_context(&self, spends: &mut Spends) -> Result<(), DriverError> {
        for (id, trade_nonce, trade_prices) in self.transfer_fee_trade_contexts()? {
            if !spends.cats.contains_key(&id) {
                // Not all transfer-fee contexts correspond to CATs being spent by this `Spends`.
                continue;
            }

            if let Some(existing) = spends.cat_transfer_fee_contexts.get(&id) {
                if existing.trade_nonce != trade_nonce || existing.trade_prices != trade_prices {
                  return Err(DriverError::InvalidTradeContext);
                }
                continue;
            }

            spends.cat_transfer_fee_contexts.insert(id, CatTransferFeeContext::new(trade_nonce, trade_prices));
        }

        Ok(())
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
            let cat_asset_info = self.asset_info.cat(asset_id);
            let cat_info = CatInfo::new(
                asset_id,
                cat_asset_info.and_then(|info| info.hidden_puzzle_hash),
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .with_transfer_fee_policy(cat_asset_info.and_then(|info| info.transfer_fee_policy.clone()));

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
        self.requested_payments.extend(other.requested_payments)?;
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

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_bls::Signature;
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzle_types::{
        Memos,
        offer::{NotarizedPayment, Payment},
    };
    use chia_sdk_test::{Simulator, sign_transaction};
    use clvmr::NodePtr;
    use indexmap::indexmap;

    use crate::{
        Action, AssetInfo, Cat, CatAssetInfo, CatInfo, Id, NftAssetInfo, OfferCoins, Relation,
        RequestedPayments, SpendContext, Spends, TransferFeeInfo, TransferFeePolicy,
    };

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

    #[test]
    fn test_offer_transfer_fee_metadata() -> anyhow::Result<()> {
        let asset_id = Bytes32::new([7; 32]);
        let fee_policy = TransferFeePolicy::new(Bytes32::new([8; 32]), 500, 1, false, false);

        let offered_cat = Cat::new(
            Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 100),
            None,
            CatInfo::new(asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
                .with_transfer_fee_policy(Some(fee_policy.clone())),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins.cats.insert(asset_id, vec![offered_cat]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.cats.insert(
            asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([2; 32]),
                vec![Payment::new(Bytes32::new([3; 32]), 100, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(asset_id, CatAssetInfo::new(None, Some(fee_policy.clone())))?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        assert_eq!(
            offer.offered_transfer_fees(),
            vec![TransferFeeInfo::new(asset_id, fee_policy.clone())]
        );
        assert_eq!(
            offer.requested_transfer_fees(),
            vec![TransferFeeInfo::new(asset_id, fee_policy)]
        );

        Ok(())
    }

    #[test]
    fn test_offer_transfer_fee_amounts() -> anyhow::Result<()> {
        let fee_asset_id = Bytes32::new([7; 32]);
        let quote_asset_id = Bytes32::new([9; 32]);
        let fee_policy = TransferFeePolicy::new(Bytes32::new([8; 32]), 500, 1, false, false);

        // Requested fee CAT => offered side pays issuer fees based on offered quote amounts.
        let offered_quote_cat = Cat::new(
            Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 200),
            None,
            CatInfo::new(quote_asset_id, None, SETTLEMENT_PAYMENT_HASH.into()),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins.xch.push(Coin::new(
            Bytes32::new([2; 32]),
            SETTLEMENT_PAYMENT_HASH.into(),
            1_000,
        ));
        offered_coins
            .cats
            .insert(quote_asset_id, vec![offered_quote_cat]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.cats.insert(
            fee_asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([3; 32]),
                vec![Payment::new(Bytes32::new([4; 32]), 1, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(
            fee_asset_id,
            CatAssetInfo::new(None, Some(fee_policy.clone())),
        )?;
        asset_info.insert_cat(quote_asset_id, CatAssetInfo::new(None, None))?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        let offered_amounts = offer.offered_transfer_fee_amounts();
        assert_eq!(offered_amounts.xch, 50);
        assert_eq!(offered_amounts.cats[&quote_asset_id], 10);
        assert!(!offered_amounts.cats.contains_key(&fee_asset_id));

        // Offered fee CAT => requested side pays issuer fees based on requested quote amounts.
        let offered_fee_cat = Cat::new(
            Coin::new(Bytes32::new([5; 32]), SETTLEMENT_PAYMENT_HASH.into(), 100),
            None,
            CatInfo::new(fee_asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
                .with_transfer_fee_policy(Some(fee_policy.clone())),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins
            .cats
            .insert(fee_asset_id, vec![offered_fee_cat.clone()]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.xch.push(NotarizedPayment::new(
            Bytes32::new([6; 32]),
            vec![Payment::new(Bytes32::new([10; 32]), 1_000, Memos::None)],
        ));
        requested_payments.cats.insert(
            quote_asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([11; 32]),
                vec![Payment::new(Bytes32::new([12; 32]), 200, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(
            fee_asset_id,
            CatAssetInfo::new(None, Some(fee_policy.clone())),
        )?;
        asset_info.insert_cat(quote_asset_id, CatAssetInfo::new(None, None))?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        let requested_amounts = offer.requested_transfer_fee_amounts();
        assert_eq!(requested_amounts.xch, 50);
        assert_eq!(requested_amounts.cats[&quote_asset_id], 10);
        assert!(!requested_amounts.cats.contains_key(&fee_asset_id));

        Ok(())
    }

    #[test]
    fn test_offer_transfer_fee_payments() -> anyhow::Result<()> {
        let fee_asset_id = Bytes32::new([7; 32]);
        let quote_asset_id = Bytes32::new([9; 32]);
        let fee_policy = TransferFeePolicy::new(Bytes32::new([8; 32]), 500, 1, false, false);

        let offered_quote_cat = Cat::new(
            Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 200),
            None,
            CatInfo::new(quote_asset_id, None, SETTLEMENT_PAYMENT_HASH.into()),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins.xch.push(Coin::new(
            Bytes32::new([2; 32]),
            SETTLEMENT_PAYMENT_HASH.into(),
            1_000,
        ));
        offered_coins
            .cats
            .insert(quote_asset_id, vec![offered_quote_cat]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.cats.insert(
            fee_asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([3; 32]),
                vec![Payment::new(Bytes32::new([4; 32]), 1, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(
            fee_asset_id,
            CatAssetInfo::new(None, Some(fee_policy.clone())),
        )?;
        asset_info.insert_cat(quote_asset_id, CatAssetInfo::new(None, None))?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        let mut ctx = SpendContext::new();
        let nonce = Bytes32::new([5; 32]);
        let fee_payments = offer.offered_transfer_fee_payments(&mut ctx, nonce)?;

        assert_eq!(fee_payments.xch.len(), 1);
        assert_eq!(fee_payments.xch[0].nonce, nonce);
        assert_eq!(fee_payments.xch[0].payments.len(), 1);
        assert_eq!(
            fee_payments.xch[0].payments[0],
            Payment::new(
                fee_policy.issuer_fee_puzzle_hash,
                50,
                Memos::Some(NodePtr::NIL)
            )
        );

        assert_eq!(fee_payments.cats[&quote_asset_id].len(), 1);
        assert_eq!(fee_payments.cats[&quote_asset_id][0].nonce, nonce);
        let cat_fee_payment = &fee_payments.cats[&quote_asset_id][0].payments[0];
        assert_eq!(cat_fee_payment.puzzle_hash, fee_policy.issuer_fee_puzzle_hash);
        assert_eq!(cat_fee_payment.amount, 10);
        let Memos::Some(memos_ptr) = cat_fee_payment.memos else {
            panic!("expected CAT fee payment memos to be present");
        };
        let hints = Vec::<Bytes32>::from_clvm(&*ctx, memos_ptr)?;
        assert_eq!(hints, vec![fee_policy.issuer_fee_puzzle_hash]);

        Ok(())
    }

    #[test]
    fn test_offer_transfer_fee_payments_allow_fee_enabled_quote_asset() -> anyhow::Result<()> {
        let fee_asset_id = Bytes32::new([7; 32]);
        let quote_fee_asset_id = Bytes32::new([9; 32]);
        let quote_fee_policy = TransferFeePolicy::new(Bytes32::new([10; 32]), 250, 1, false, false);
        let fee_policy = TransferFeePolicy::new(Bytes32::new([8; 32]), 500, 1, false, false);

        let offered_quote_fee_cat = Cat::new(
            Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 200),
            None,
            CatInfo::new(quote_fee_asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
                .with_transfer_fee_policy(Some(quote_fee_policy.clone())),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins
            .cats
            .insert(quote_fee_asset_id, vec![offered_quote_fee_cat]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.cats.insert(
            fee_asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([2; 32]),
                vec![Payment::new(Bytes32::new([3; 32]), 1, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(
            fee_asset_id,
            CatAssetInfo::new(None, Some(fee_policy.clone())),
        )?;
        asset_info.insert_cat(
            quote_fee_asset_id,
            CatAssetInfo::new(None, Some(quote_fee_policy)),
        )?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        let mut ctx = SpendContext::new();
        let nonce = Bytes32::new([4; 32]);
        let fee_payments = offer.offered_transfer_fee_payments(&mut ctx, nonce)?;
        assert_eq!(fee_payments.cats[&quote_fee_asset_id].len(), 1);
        assert_eq!(fee_payments.cats[&quote_fee_asset_id][0].nonce, nonce);
        let cat_fee_payment = &fee_payments.cats[&quote_fee_asset_id][0].payments[0];
        assert_eq!(cat_fee_payment.puzzle_hash, fee_policy.issuer_fee_puzzle_hash);
        assert_eq!(cat_fee_payment.amount, 10);
        let Memos::Some(memos_ptr) = cat_fee_payment.memos else {
            panic!("expected CAT fee payment memos to be present");
        };
        let hints = Vec::<Bytes32>::from_clvm(&*ctx, memos_ptr)?;
        assert_eq!(hints, vec![fee_policy.issuer_fee_puzzle_hash]);

        Ok(())
    }

    #[test]
    fn test_offer_requested_transfer_fee_payments_allow_fee_enabled_quote_asset()
    -> anyhow::Result<()> {
        let fee_asset_id = Bytes32::new([7; 32]);
        let quote_fee_asset_id = Bytes32::new([9; 32]);
        let fee_policy = TransferFeePolicy::new(Bytes32::new([8; 32]), 500, 1, false, false);
        let quote_fee_policy = TransferFeePolicy::new(Bytes32::new([10; 32]), 250, 1, false, false);

        let offered_fee_cat = Cat::new(
            Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 1),
            None,
            CatInfo::new(fee_asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
                .with_transfer_fee_policy(Some(fee_policy.clone())),
        );

        let mut offered_coins = OfferCoins::new();
        offered_coins
            .cats
            .insert(fee_asset_id, vec![offered_fee_cat]);

        let mut requested_payments = RequestedPayments::new();
        requested_payments.cats.insert(
            quote_fee_asset_id,
            vec![NotarizedPayment::new(
                Bytes32::new([2; 32]),
                vec![Payment::new(Bytes32::new([3; 32]), 200, Memos::None)],
            )],
        );

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(fee_asset_id, CatAssetInfo::new(None, Some(fee_policy)))?;
        asset_info.insert_cat(
            quote_fee_asset_id,
            CatAssetInfo::new(None, Some(quote_fee_policy)),
        )?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        let mut ctx = SpendContext::new();
        let nonce = Bytes32::new([4; 32]);
        let fee_payments = offer.requested_transfer_fee_payments(&mut ctx, nonce)?;
        assert_eq!(fee_payments.cats[&quote_fee_asset_id].len(), 1);
        assert_eq!(fee_payments.cats[&quote_fee_asset_id][0].nonce, nonce);
        let cat_fee_payment = &fee_payments.cats[&quote_fee_asset_id][0].payments[0];
        assert_eq!(cat_fee_payment.puzzle_hash, fee_policy.issuer_fee_puzzle_hash);
        assert_eq!(cat_fee_payment.amount, 10);
        let Memos::Some(memos_ptr) = cat_fee_payment.memos else {
            panic!("expected CAT fee payment memos to be present");
        };
        let hints = Vec::<Bytes32>::from_clvm(&*ctx, memos_ptr)?;
        assert_eq!(hints, vec![fee_policy.issuer_fee_puzzle_hash]);

        Ok(())
    }
}
