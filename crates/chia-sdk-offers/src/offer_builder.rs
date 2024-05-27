use chia_bls::Signature;
use chia_protocol::{Bytes32, CoinSpend, Program};
use chia_puzzles::offer::{NotarizedPayment, Payment};
use chia_sdk_driver::{
    spend_builder::{P2Spend, SpendConditions},
    SpendContext, SpendError,
};

use clvm_traits::ToNodePtr;
use clvm_utils::ToTreeHash;
use indexmap::IndexMap;

use crate::Offer;

#[derive(Debug, Clone)]
pub struct RequestPayments {
    nonce: Bytes32,
    required_conditions: SpendConditions,
    requested_payments: IndexMap<Program, Vec<NotarizedPayment>>,
}

#[derive(Debug, Clone)]
pub struct MakePayments {
    requested_payments: IndexMap<Program, Vec<NotarizedPayment>>,
}

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct OfferBuilder<T> {
    state: T,
}

impl OfferBuilder<RequestPayments> {
    pub fn new(mut offered_coin_ids: Vec<Bytes32>) -> Self {
        offered_coin_ids.sort();

        Self {
            state: RequestPayments {
                nonce: offered_coin_ids.tree_hash().into(),
                required_conditions: SpendConditions::new(),
                requested_payments: IndexMap::new(),
            },
        }
    }

    pub fn request_standard_payments(
        self,
        ctx: &mut SpendContext<'_>,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let puzzle = ctx.settlement_payments_puzzle()?;
        self.request_raw_payments(ctx, &puzzle, payments)
    }

    pub fn request_raw_payments<P>(
        mut self,
        ctx: &mut SpendContext<'_>,
        puzzle: &P,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError>
    where
        P: ToNodePtr,
    {
        let puzzle_ptr = ctx.alloc(puzzle)?;
        let puzzle_hash = ctx.tree_hash(puzzle_ptr).into();
        let puzzle_reveal = ctx.serialize(&puzzle_ptr)?;

        let notarized_payment = NotarizedPayment {
            nonce: self.state.nonce,
            payments,
        };

        self.state
            .requested_payments
            .entry(puzzle_reveal)
            .or_default()
            .extend([notarized_payment.clone()]);

        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

        self.state.required_conditions = self
            .state
            .required_conditions
            .assert_puzzle_announcement(ctx, puzzle_hash, notarized_payment_hash)?;

        Ok(self)
    }

    pub fn make_payments(self) -> (SpendConditions, OfferBuilder<MakePayments>) {
        let builder = OfferBuilder {
            state: MakePayments {
                requested_payments: self.state.requested_payments,
            },
        };

        (self.state.required_conditions, builder)
    }
}

impl OfferBuilder<MakePayments> {
    pub fn finish(
        self,
        offered_coin_spends: Vec<CoinSpend>,
        aggregated_signature: Signature,
    ) -> Result<Offer, SpendError> {
        Ok(Offer::new(
            self.state.requested_payments,
            offered_coin_spends,
            aggregated_signature,
        ))
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Coin, SpendBundle};
    use chia_puzzles::{
        offer::{PaymentWithoutMemos, SETTLEMENT_PAYMENTS_PUZZLE_HASH},
        standard::StandardArgs,
    };
    use chia_sdk_driver::puzzles::StandardSpend;
    use chia_sdk_test::{sign_transaction, Simulator};
    use clvmr::Allocator;

    use crate::SettlementSpend;

    use super::*;

    #[tokio::test]
    async fn test_simple_offer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let a_secret_key = sim.secret_key().await?;
        let a_public_key = a_secret_key.public_key();
        let a_puzzle_hash = StandardArgs::curry_tree_hash(a_public_key).into();

        let b_secret_key = sim.secret_key().await?;
        let b_public_key = b_secret_key.public_key();
        let b_puzzle_hash = StandardArgs::curry_tree_hash(b_public_key).into();

        let a = sim.mint_coin(a_puzzle_hash, 1000).await;
        let b = sim.mint_coin(b_puzzle_hash, 3000).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (a_conditions, partial_offer) = OfferBuilder::new(vec![a.coin_id()])
            .request_standard_payments(
                ctx,
                vec![Payment::WithoutMemos(PaymentWithoutMemos {
                    puzzle_hash: a_puzzle_hash,
                    amount: b.amount,
                })],
            )?
            .make_payments();

        StandardSpend::new()
            .chain(a_conditions)
            .create_coin(ctx, SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), a.amount)?
            .finish(ctx, a, a_public_key)?;

        let coin_spends = ctx.take_spends();
        let signature = sign_transaction(&coin_spends, &[a_secret_key])?;
        let a_offer = partial_offer.finish(coin_spends, signature)?;

        let (b_conditions, partial_offer) = OfferBuilder::new(vec![b.coin_id()])
            .request_standard_payments(
                ctx,
                vec![Payment::WithoutMemos(PaymentWithoutMemos {
                    puzzle_hash: b_puzzle_hash,
                    amount: a.amount,
                })],
            )?
            .make_payments();

        StandardSpend::new()
            .chain(b_conditions)
            .create_coin(ctx, SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), b.amount)?
            .finish(ctx, b, b_public_key)?;

        let coin_spends = ctx.take_spends();
        let signature = sign_transaction(&coin_spends, &[b_secret_key])?;
        let b_offer = partial_offer.finish(coin_spends, signature)?;

        SettlementSpend::new(
            b_offer
                .requested_payments()
                .values()
                .next()
                .cloned()
                .unwrap(),
        )
        .finish(
            ctx,
            Coin::new(
                a.coin_id(),
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                a.amount,
            ),
        )?;

        SettlementSpend::new(
            a_offer
                .requested_payments()
                .values()
                .next()
                .cloned()
                .unwrap(),
        )
        .finish(
            ctx,
            Coin::new(
                b.coin_id(),
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                b.amount,
            ),
        )?;

        let spend_bundle = SpendBundle::new(
            [
                a_offer.offered_coin_spends().to_vec(),
                b_offer.offered_coin_spends().to_vec(),
                ctx.take_spends(),
            ]
            .concat(),
            a_offer.aggregated_signature() + b_offer.aggregated_signature(),
        );

        println!("{SETTLEMENT_PAYMENTS_PUZZLE_HASH}");
        println!("A COIN: {a:?}");
        println!("B COIN: {b:?}");
        dbg!(&spend_bundle);

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }
}
