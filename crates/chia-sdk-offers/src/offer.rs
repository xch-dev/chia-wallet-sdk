use chia_bls::Signature;
use chia_protocol::{CoinSpend, Program, SpendBundle};
use chia_puzzles::offer::NotarizedPayment;
use chia_sdk_driver::{SpendContext, SpendError};
use indexmap::IndexMap;

use crate::{parse_payments, payment_coin_spend};

#[derive(Debug)]
pub struct Offer {
    requested_payments: IndexMap<Program, Vec<NotarizedPayment>>,
    offered_coin_spends: Vec<CoinSpend>,
    aggregated_signature: Signature,
}

impl Offer {
    pub fn new(
        requested_payments: IndexMap<Program, Vec<NotarizedPayment>>,
        offered_coin_spends: Vec<CoinSpend>,
        aggregated_signature: Signature,
    ) -> Self {
        Self {
            requested_payments,
            offered_coin_spends,
            aggregated_signature,
        }
    }

    pub fn from_spend_bundle(
        ctx: &mut SpendContext,
        spend_bundle: SpendBundle,
    ) -> Result<Self, SpendError> {
        let mut requested_payments = IndexMap::<Program, Vec<NotarizedPayment>>::new();
        let mut offered_coin_spends = Vec::new();

        for coin_spend in spend_bundle.coin_spends {
            let Some(notarized_payments) = parse_payments(ctx, &coin_spend)? else {
                offered_coin_spends.push(coin_spend);
                continue;
            };

            requested_payments
                .entry(coin_spend.puzzle_reveal)
                .or_default()
                .extend(notarized_payments);
        }

        Ok(Self {
            requested_payments,
            offered_coin_spends,
            aggregated_signature: spend_bundle.aggregated_signature,
        })
    }

    pub fn into_spend_bundle(self, ctx: &mut SpendContext) -> Result<SpendBundle, SpendError> {
        let mut coin_spends = self.offered_coin_spends;

        for (puzzle_reveal, notarized_payments) in self.requested_payments {
            let coin_spend = payment_coin_spend(ctx, &puzzle_reveal, notarized_payments)?;
            coin_spends.push(coin_spend);
        }

        Ok(SpendBundle::new(coin_spends, self.aggregated_signature))
    }

    pub fn requested_payments(&self) -> &IndexMap<Program, Vec<NotarizedPayment>> {
        &self.requested_payments
    }

    pub fn offered_coin_spends(&self) -> &[CoinSpend] {
        &self.offered_coin_spends
    }

    pub fn aggregated_signature(&self) -> &Signature {
        &self.aggregated_signature
    }
}
