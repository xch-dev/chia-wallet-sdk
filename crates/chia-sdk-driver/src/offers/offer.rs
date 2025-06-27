use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{puzzles::SettlementPayment, run_puzzle, Condition};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::Allocator;
use indexmap::IndexSet;

use crate::{
    calculate_royalty_amounts, calculate_trade_price_amounts, Arbitrage, AssetInfo, CatInfo,
    DriverError, Layer, NftInfo, OfferAmounts, OfferCoins, OptionInfo, Puzzle, RequestedPayments,
    RoyaltyInfo, SpendContext,
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

    pub fn offered_royalty_amounts(&self) -> OfferAmounts {
        let offered_amounts = self.offered_coins.amounts();
        let royalties = self.offered_royalties();
        let trade_prices = calculate_trade_price_amounts(&offered_amounts, royalties.len());
        calculate_royalty_amounts(&trade_prices, &royalties)
    }

    pub fn requested_royalty_amounts(&self) -> OfferAmounts {
        let requested_amounts = self.requested_payments.amounts();
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
                arbitrage.requested_nfts.push(launcher_id);
            } else if !is_offered && is_requested {
                arbitrage.offered_nfts.push(launcher_id);
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
    use chia_puzzle_types::{
        offer::{NotarizedPayment, Payment},
        Memos,
    };
    use chia_sdk_test::{sign_transaction, Simulator};
    use indexmap::indexmap;

    use crate::{Action, Id, NftAssetInfo, Relation, SpendContext, Spends};

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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

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
