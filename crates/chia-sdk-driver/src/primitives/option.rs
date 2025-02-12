use chia_protocol::Bytes32;
use chia_puzzles::offer::{NotarizedPayment, Payment, SETTLEMENT_PAYMENTS_PUZZLE_HASH};
use chia_sdk_types::{Conditions, Memos};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;
use hex_literal::hex;

use crate::{
    payment_assertion, DriverError, HashedPtr, Layer, Make, Offer, OfferBuilder,
    P2ConditionsOptionsArgs, SpendContext,
};

use super::NftInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionContract<M> {
    pub nft_info: NftInfo<M>,
    pub p2_puzzle_hash: Bytes32,
    pub expiration_seconds: u64,
}

impl<M> OptionContract<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    /// Creates the p2 option puzzle hash, used to lock up the option coins.
    pub fn p2_option_puzzle(
        &self,
        ctx: &mut SpendContext,
        offered_amount: u64,
        assertions: Conditions<HashedPtr>,
        include_hint: bool,
    ) -> Result<P2ConditionsOptionsArgs<HashedPtr>, DriverError> {
        let settlement_payments = ctx.settlement_payments_puzzle()?;
        let nft_puzzle = self
            .nft_info
            .clone()
            .into_layers(settlement_payments)
            .construct_puzzle(ctx)?;
        let nft_puzzle_hash = ctx.tree_hash(nft_puzzle);

        let burn_nft_assertion = payment_assertion(
            nft_puzzle_hash.into(),
            &NotarizedPayment {
                nonce: self.nft_info.launcher_id,
                payments: vec![Payment::with_memos(
                    BURN_PUZZLE_HASH,
                    1,
                    vec![BURN_PUZZLE_HASH.into()],
                )],
            },
        );

        let pre_conditions = Conditions::<HashedPtr>::default()
            .assert_before_seconds_absolute(self.expiration_seconds)
            .create_coin(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), offered_amount, None)
            .with(burn_nft_assertion)
            .extend(assertions);

        let hint = ctx.hint(self.p2_puzzle_hash)?;

        let post_conditions = Conditions::<HashedPtr>::default()
            .assert_seconds_absolute(self.expiration_seconds)
            .create_coin(
                self.p2_puzzle_hash,
                offered_amount,
                if include_hint {
                    Some(Memos::new(HashedPtr::from_ptr(&ctx.allocator, hint.value)))
                } else {
                    None
                },
            );

        Ok(P2ConditionsOptionsArgs::new(vec![
            pre_conditions,
            post_conditions,
        ]))
    }

    pub fn make_offer(
        &self,
        ctx: &mut SpendContext,
        offered_coin_id: Bytes32,
    ) -> Result<OfferBuilder<Make>, DriverError> {
        let nonce = Offer::nonce(vec![offered_coin_id]);
        let builder = OfferBuilder::new(nonce);

        let settlement_payments = ctx.settlement_payments_puzzle()?;
        let nft_puzzle = self
            .nft_info
            .clone()
            .into_layers(settlement_payments)
            .construct_puzzle(ctx)?;

        builder.request(
            ctx,
            &nft_puzzle,
            vec![Payment::with_memos(
                BURN_PUZZLE_HASH,
                1,
                vec![BURN_PUZZLE_HASH.into()],
            )],
        )
    }
}

const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

#[cfg(test)]
mod tests {
    use chia_protocol::{Coin, CoinState, SpendBundle};
    use chia_puzzles::{nft::NftMetadata, offer::SettlementPaymentsSolution};
    use chia_sdk_test::{sign_transaction, Simulator};
    use chia_sdk_types::Mod;
    use indexmap::indexset;

    use crate::{
        Cat, CatSpend, IntermediateLauncher, Launcher, NftMint, P2ConditionsOptionsLayer,
        SettlementLayer, SpendWithConditions, StandardLayer,
    };

    use super::*;

    #[test]
    #[allow(clippy::similar_names)]
    fn test_option_contract() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();
        let (sk, pk, p2_puzzle_hash, coin) = sim.child_p2(1000, 0)?;
        let (other_sk, other_pk, other_p2_puzzle_hash, other_coin) = sim.child_p2(3, 1)?;
        let p2 = StandardLayer::new(pk);
        let other_p2 = StandardLayer::new(other_pk);

        let (create_did, did) =
            Launcher::new(other_coin.coin_id(), 1).create_simple_did(ctx, &other_p2)?;

        let mint = NftMint::new(NftMetadata::default(), other_p2_puzzle_hash, 0, None);

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, mint)?;
        let _did = did.update(ctx, &other_p2, mint_nft)?;

        let memos = ctx.hint(p2_puzzle_hash)?;
        let (issue_cat, cat) = Cat::single_issuance_eve(
            ctx,
            coin.coin_id(),
            1000,
            Conditions::new().create_coin(p2_puzzle_hash, 1000, Some(memos)),
        )?;

        let cat = cat.wrapped_child(p2_puzzle_hash, 1000);

        let option_contract = OptionContract {
            nft_info: nft.info.clone(),
            p2_puzzle_hash,
            expiration_seconds: 3,
        };

        let settlement_payments = ctx.settlement_payments_puzzle()?;
        let builder = option_contract
            .make_offer(ctx, cat.coin.coin_id())?
            .request(
                ctx,
                &settlement_payments,
                vec![Payment::new(p2_puzzle_hash, 1)],
            )?;

        let expected_nonce = builder.nonce();
        let p2_option_puzzle = option_contract.p2_option_puzzle(
            ctx,
            1000,
            Conditions::default().with(payment_assertion(
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                &NotarizedPayment {
                    nonce: expected_nonce,
                    payments: vec![Payment::new(p2_puzzle_hash, 1)],
                },
            )),
            false,
        )?;
        let p2_option_puzzle_hash = p2_option_puzzle.curry_tree_hash().into();

        let inner_spend = p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(p2_option_puzzle_hash, 1000, None),
        )?;
        Cat::spend_all(ctx, &[CatSpend::new(cat, inner_spend)])?;

        p2.spend(ctx, coin, issue_cat)?;
        other_p2.spend(
            ctx,
            other_coin,
            create_did.create_coin(other_p2_puzzle_hash, 1, None),
        )?;
        sim.spend_coins(ctx.take(), &[sk.clone(), other_sk.clone()])?;

        let option_cat = cat.wrapped_child(p2_option_puzzle_hash, 1000);
        let option_layer = P2ConditionsOptionsLayer::new(p2_option_puzzle.options);
        let option_spend = option_layer.inner_spend(ctx, 0)?;
        Cat::spend_all(ctx, &[CatSpend::new(option_cat, option_spend)])?;

        let (_assertions, builder) = builder.finish();
        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[sk.clone()])?;

        let mut builder = builder.take(SpendBundle::new(coin_spends, signature));
        let _ = builder.fulfill().unwrap();
        let _ = builder.fulfill().unwrap();

        let settlement_nft = nft.lock_settlement(ctx, &other_p2, Vec::new(), Conditions::new())?;
        let nonce = settlement_nft.info.launcher_id;
        let burnt_nft = settlement_nft.unlock_settlement(
            ctx,
            vec![NotarizedPayment {
                nonce,
                payments: vec![Payment::with_memos(
                    BURN_PUZZLE_HASH,
                    1,
                    vec![BURN_PUZZLE_HASH.into()],
                )],
            }],
        )?;

        let settlement_cat = option_cat.wrapped_child(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), 1000);
        let settlement_spend = SettlementLayer.construct_spend(
            ctx,
            SettlementPaymentsSolution {
                notarized_payments: vec![NotarizedPayment {
                    nonce: Bytes32::default(),
                    payments: vec![Payment::with_memos(
                        other_p2_puzzle_hash,
                        1000,
                        vec![other_p2_puzzle_hash.into()],
                    )],
                }],
            },
        )?;
        Cat::spend_all(ctx, &[CatSpend::new(settlement_cat, settlement_spend)])?;

        let coin = Coin::new(other_coin.coin_id(), other_p2_puzzle_hash, 1);
        other_p2.spend(
            ctx,
            coin,
            Conditions::new().create_coin(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), 1, None),
        )?;
        let settlement_coin = Coin::new(coin.coin_id(), SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), 1);
        let coin_spend = SettlementLayer.construct_coin_spend(
            ctx,
            settlement_coin,
            SettlementPaymentsSolution {
                notarized_payments: vec![NotarizedPayment {
                    nonce: expected_nonce,
                    payments: vec![Payment::new(p2_puzzle_hash, 1)],
                }],
            },
        )?;
        ctx.insert(coin_spend);

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[sk, other_sk])?;
        let spend_bundle = builder.bundle(SpendBundle::new(coin_spends, signature));

        sim.new_transaction(spend_bundle)?;

        // todo: use lookup puzzle hashes
        let new_cat = settlement_cat.wrapped_child(other_p2_puzzle_hash, 1000);
        assert_eq!(
            sim.hinted_coins(other_p2_puzzle_hash),
            [new_cat.coin.coin_id()]
        );

        let new_hinted: Vec<CoinState> = sim
            .lookup_puzzle_hashes(indexset![p2_puzzle_hash], false)
            .into_iter()
            .filter(|cs| cs.spent_height.is_none())
            .collect();
        assert_eq!(new_hinted.len(), 1);
        let puzzle_hash = new_hinted[0].coin.puzzle_hash;
        assert_eq!(puzzle_hash, p2_puzzle_hash);

        assert!(sim.coin_state(burnt_nft.coin.coin_id()).is_some());

        Ok(())
    }
}
