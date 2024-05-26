use chia_bls::Signature;
use chia_protocol::{Bytes32, CoinSpend, SpendBundle};
use chia_sdk_driver::{SpendContext, SpendError};
use indexmap::IndexMap;

use crate::RequestedPayments;

#[derive(Debug)]
pub struct Offer {
    requested_payments: IndexMap<Bytes32, RequestedPayments>,
    offered_coin_spends: Vec<CoinSpend>,
    aggregated_signature: Signature,
}

impl Offer {
    pub fn from_spend_bundle(
        ctx: &mut SpendContext<'_>,
        spend_bundle: SpendBundle,
    ) -> Result<Self, SpendError> {
        let mut requested_payments = IndexMap::<Bytes32, RequestedPayments>::new();
        let mut offered_coin_spends = Vec::new();

        for coin_spend in spend_bundle.coin_spends {
            if let Some(requested) = RequestedPayments::from_coin_spend(ctx, coin_spend.clone())? {
                if let Some(existing) = requested_payments.get_mut(&requested.puzzle_hash()) {
                    existing.extend(requested.notarized_payments().to_vec());
                } else {
                    requested_payments.insert(requested.puzzle_hash(), requested);
                }
            } else {
                offered_coin_spends.push(coin_spend);
            }
        }

        Ok(Self {
            requested_payments,
            offered_coin_spends,
            aggregated_signature: spend_bundle.aggregated_signature,
        })
    }

    pub fn into_spend_bundle(self, ctx: &mut SpendContext<'_>) -> Result<SpendBundle, SpendError> {
        let mut coin_spends = self.offered_coin_spends;

        for requested_payments in self.requested_payments.into_values() {
            let coin_spend = requested_payments.into_coin_spend(ctx)?;
            coin_spends.push(coin_spend);
        }

        Ok(SpendBundle::new(coin_spends, self.aggregated_signature))
    }

    pub fn requested_payments(&self) -> &IndexMap<Bytes32, RequestedPayments> {
        &self.requested_payments
    }

    pub fn offered_coin_spends(&self) -> &[CoinSpend] {
        &self.offered_coin_spends
    }
}
