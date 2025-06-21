use std::collections::HashSet;

use chia_bls::{sign, SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_signer::{AggSigConstants, RequiredSignature};
use chia_sdk_types::puzzles::{P2DelegatedConditionsSolution, SettlementPayment};
use clvm_traits::ToClvm;
use clvm_utils::ToTreeHash;
use clvmr::Allocator;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::{
    Action, AssetInfo, CatInfo, ConditionsSpend, DriverError, Id, Layer, NftInfo, OfferCoins,
    OptionInfo, Outputs, P2DelegatedConditionsLayer, Puzzle, Relation, RequestedPayments,
    SettlementLayer, Spend, SpendContext, SpendKind, Spends,
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

    pub fn offered_coins(&self) -> &OfferCoins {
        &self.offered_coins
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
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

    pub fn take(
        self,
        ctx: &mut SpendContext,
        arbitrage_puzzle_hash: Bytes32,
        constants: &AggSigConstants,
    ) -> Result<(SpendBundle, Outputs), DriverError> {
        let mut rng = ChaCha20Rng::from_entropy();
        let seed: [u8; 32] = rng.gen();

        let intermediate_secret_key = SecretKey::from_seed(&seed);
        let intermediate_puzzle =
            P2DelegatedConditionsLayer::new(intermediate_secret_key.public_key());
        let intermediate_puzzle_hash = intermediate_puzzle.tree_hash().into();

        self.take_with_intermediate(
            ctx,
            intermediate_puzzle_hash,
            |ctx, spend| {
                intermediate_puzzle.construct_spend(
                    ctx,
                    P2DelegatedConditionsSolution::new(spend.finish().into_vec()),
                )
            },
            |coin_spends| {
                let mut allocator = Allocator::new();
                let mut signature = Signature::default();

                for required in
                    RequiredSignature::from_coin_spends(&mut allocator, &coin_spends, constants)?
                {
                    let RequiredSignature::Bls(required) = required else {
                        continue;
                    };

                    if required.public_key == intermediate_puzzle.public_key {
                        signature += &sign(&intermediate_secret_key, required.message());
                    }
                }

                Ok(SpendBundle::new(coin_spends, signature))
            },
            arbitrage_puzzle_hash,
        )
    }

    pub fn take_with_intermediate(
        self,
        ctx: &mut SpendContext,
        intermediate_puzzle_hash: Bytes32,
        intermediate_spend: impl Fn(&mut SpendContext, ConditionsSpend) -> Result<Spend, DriverError>,
        sign_spends: impl Fn(Vec<CoinSpend>) -> Result<SpendBundle, DriverError>,
        arbitrage_puzzle_hash: Bytes32,
    ) -> Result<(SpendBundle, Outputs), DriverError> {
        let mut spends = Spends::with_separate_change_puzzle_hash(
            intermediate_puzzle_hash,
            arbitrage_puzzle_hash,
        );

        spends.add(self.offered_coins);
        spends.conditions.disable_settlement_assertions = true;

        let mut actions = Vec::new();

        for (id, notarized_payment) in
            self.requested_payments
                .xch
                .into_iter()
                .map(|np| (Id::Xch, np))
                .chain(
                    self.requested_payments
                        .cats
                        .into_iter()
                        .flat_map(|(asset_id, cat)| {
                            cat.into_iter().map(move |np| (Id::Existing(asset_id), np))
                        }),
                )
                .chain(
                    self.requested_payments
                        .nfts
                        .into_iter()
                        .flat_map(|(launcher_id, nft)| {
                            nft.into_iter()
                                .map(move |np| (Id::Existing(launcher_id), np))
                        }),
                )
                .chain(self.requested_payments.options.into_iter().flat_map(
                    |(launcher_id, option)| {
                        option
                            .into_iter()
                            .map(move |np| (Id::Existing(launcher_id), np))
                    },
                ))
        {
            actions.push(Action::settle(id, notarized_payment));
        }

        let deltas = spends.apply(ctx, &actions)?;
        let outputs = spends.finish(
            ctx,
            &deltas,
            Relation::AssertConcurrent,
            |ctx, _, spend| match spend {
                SpendKind::Conditions(spend) => intermediate_spend(ctx, spend),
                SpendKind::Settlement(spend) => SettlementLayer
                    .construct_spend(ctx, SettlementPaymentsSolution::new(spend.finish())),
            },
        )?;

        let spend_bundle = sign_spends(ctx.take())?;

        let spend_bundle = SpendBundle::new(
            [self.spend_bundle.coin_spends, spend_bundle.coin_spends].concat(),
            self.spend_bundle.aggregated_signature + &spend_bundle.aggregated_signature,
        );

        Ok((spend_bundle, outputs))
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::{
        offer::{NotarizedPayment, Payment},
        Memos,
    };
    use chia_sdk_test::{sign_transaction, Simulator};
    use chia_sdk_types::TESTNET11_CONSTANTS;
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

        let mut offer = Offer::from_input_spend_bundle(
            &mut ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            requested_asset_info,
        )?;

        // Take offer
        let mut requested_payments = RequestedPayments::new();
        let mut requested_asset_info = AssetInfo::new();

        requested_payments.nfts.insert(
            alice_nft.info.launcher_id,
            vec![NotarizedPayment::new(
                Offer::nonce(vec![bob_nft.coin.coin_id()]),
                vec![Payment::new(bob.puzzle_hash, 1, bob_hint)],
            )],
        );
        requested_asset_info.insert_nft(
            alice_nft.info.launcher_id,
            NftAssetInfo::new(
                alice_nft.info.metadata,
                alice_nft.info.metadata_updater_puzzle_hash,
                alice_nft.info.royalty_puzzle_hash,
                alice_nft.info.royalty_basis_points,
            ),
        )?;

        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(bob_nft);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::send(
                Id::Existing(bob_nft.info.launcher_id),
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
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[bob.sk])?;

        let take_offer = Offer::from_input_spend_bundle(
            &mut ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            requested_asset_info,
        )?;

        offer.extend(take_offer)?;

        let (spend_bundle, outputs) = offer.take(
            &mut ctx,
            bob.puzzle_hash,
            &AggSigConstants::new(TESTNET11_CONSTANTS.agg_sig_me_additional_data),
        )?;

        sim.new_transaction(spend_bundle)?;

        let final_bob_nft = outputs.nfts[&Id::Existing(alice_nft.info.launcher_id)];
        let final_alice_nft = outputs.nfts[&Id::Existing(bob_nft.info.launcher_id)];

        assert_eq!(final_bob_nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(final_alice_nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }
}
