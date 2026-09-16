use chia_consensus::{conditions::ELIGIBLE_FOR_DEDUP, validation_error::ErrorCode};
use chia_protocol::{Bytes32, SpendBundle};
use chia_sdk_coinset::PushTxResponse;
use indexmap::IndexSet;

use crate::SimulatorError;

use super::{
    FullNodeSimulator, FullNodeSimulatorPushTxResponse, ValidatedBundle, ValidatedSpend,
    fast_forward::FastForwardResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NormalizationState {
    ordered_coin_ids: Vec<Bytes32>,
    spend_bundle_id: Bytes32,
}

impl NormalizationState {
    fn new(spend_bundle: &SpendBundle) -> Self {
        Self {
            ordered_coin_ids: spend_bundle
                .coin_spends
                .iter()
                .map(|coin_spend| coin_spend.coin.coin_id())
                .collect(),
            spend_bundle_id: spend_bundle.name(),
        }
    }
}

#[derive(Debug, Default)]
struct NormalizationProgress {
    seen: IndexSet<NormalizationState>,
}

impl NormalizationProgress {
    fn record(&mut self, spend_bundle: &SpendBundle) -> bool {
        self.seen.insert(NormalizationState::new(spend_bundle))
    }
}

impl FullNodeSimulator {
    fn insert_mempool_item(
        &mut self,
        tx_id: Bytes32,
        validated: ValidatedBundle,
    ) -> Result<(), SimulatorError> {
        let conflicting_tx_ids = self.conflicting_mempool_tx_ids(&validated);
        if !conflicting_tx_ids.is_empty()
            && !self.is_mempool_replacement(&validated, &conflicting_tx_ids)
        {
            return Err(SimulatorError::Validation(ErrorCode::MempoolConflict));
        }
        if !conflicting_tx_ids.is_empty() {
            for tx_id in conflicting_tx_ids {
                self.mempool.swap_remove(&tx_id);
            }
        }

        self.mempool.insert(tx_id, validated);
        Ok(())
    }

    fn mempool_rejects(&self, validated: &ValidatedBundle) -> bool {
        let conflicting_tx_ids = self.conflicting_mempool_tx_ids(validated);
        !conflicting_tx_ids.is_empty()
            && !self.is_mempool_replacement(validated, &conflicting_tx_ids)
    }

    fn is_mempool_replacement(
        &self,
        validated: &ValidatedBundle,
        conflicting_tx_ids: &[Bytes32],
    ) -> bool {
        let conflicting_removals = conflicting_tx_ids
            .iter()
            .filter_map(|tx_id| self.mempool.get(tx_id))
            .flat_map(|item| item.removals.iter().copied())
            .collect::<IndexSet<_>>();
        let conflicting_fees = conflicting_tx_ids
            .iter()
            .filter_map(|tx_id| self.mempool.get(tx_id))
            .map(|item| item.fee)
            .sum::<u64>();

        conflicting_removals
            .iter()
            .all(|coin_id| validated.removals.contains(coin_id))
            && validated.fee > conflicting_fees
    }

    fn conflicting_mempool_tx_ids(&self, validated: &ValidatedBundle) -> Vec<Bytes32> {
        self.mempool
            .iter()
            .filter(|(_, item)| Self::has_non_dedup_overlap(validated, item))
            .map(|(tx_id, _)| *tx_id)
            .collect()
    }

    fn has_non_dedup_overlap(lhs: &ValidatedBundle, rhs: &ValidatedBundle) -> bool {
        lhs.removals.iter().any(|coin_id| {
            rhs.removals.contains(coin_id) && !Self::removal_is_dedup_compatible(lhs, rhs, *coin_id)
        })
    }

    fn removal_is_dedup_compatible(
        lhs: &ValidatedBundle,
        rhs: &ValidatedBundle,
        coin_id: Bytes32,
    ) -> bool {
        let Some(lhs_spend) = lhs.spends.get(&coin_id) else {
            return false;
        };
        let Some(rhs_spend) = rhs.spends.get(&coin_id) else {
            return false;
        };
        Self::spends_are_dedup_compatible(lhs_spend, rhs_spend)
    }

    pub(super) fn spends_are_dedup_compatible(lhs: &ValidatedSpend, rhs: &ValidatedSpend) -> bool {
        (lhs.flags & ELIGIBLE_FOR_DEDUP) != 0
            && (rhs.flags & ELIGIBLE_FOR_DEDUP) != 0
            && lhs.fingerprint.is_some()
            && lhs.fingerprint == rhs.fingerprint
    }

    fn push_tx_success() -> FullNodeSimulatorPushTxResponse {
        FullNodeSimulatorPushTxResponse {
            response: PushTxResponse {
                status: Some("SUCCESS".to_string()),
                error: None,
                success: true,
            },
            error: None,
        }
    }

    fn push_tx_failure(error: SimulatorError) -> FullNodeSimulatorPushTxResponse {
        FullNodeSimulatorPushTxResponse {
            response: PushTxResponse {
                status: Some("FAILED".to_string()),
                error: Some(error.to_string()),
                success: false,
            },
            error: Some(error),
        }
    }

    pub fn push_tx(&mut self, spend_bundle: SpendBundle) -> PushTxResponse {
        self.push_tx_detailed(spend_bundle).response
    }

    pub fn push_tx_detailed(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> FullNodeSimulatorPushTxResponse {
        match self.normalize_and_insert(spend_bundle) {
            Ok(()) => Self::push_tx_success(),
            Err(error) => Self::push_tx_failure(error),
        }
    }

    pub(super) fn normalize_and_insert(
        &mut self,
        mut spend_bundle: SpendBundle,
    ) -> Result<(), SimulatorError> {
        let mut progress = NormalizationProgress::default();
        let mut cycle_error = ErrorCode::DoubleSpend;

        loop {
            let tx_id = spend_bundle.name();
            if self.mempool.contains_key(&tx_id) {
                return Ok(());
            }
            if !progress.record(&spend_bundle) {
                return Err(SimulatorError::Validation(cycle_error));
            }

            if let FastForwardResult::Rewritten(rewritten) =
                self.fast_forward_settled_spends(&spend_bundle)
            {
                cycle_error = ErrorCode::DoubleSpend;
                spend_bundle = *rewritten;
                continue;
            }

            let validated = self.validate_bundle(spend_bundle)?;
            if self.mempool_rejects(&validated) {
                match self.fast_forward_mempool_spends(&validated) {
                    FastForwardResult::Rewritten(rewritten) => {
                        cycle_error = ErrorCode::MempoolConflict;
                        spend_bundle = *rewritten;
                        continue;
                    }
                    FastForwardResult::NoProgress => {
                        return Err(SimulatorError::Validation(ErrorCode::MempoolConflict));
                    }
                }
            }

            return self.insert_mempool_item(tx_id, validated);
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_protocol::{Coin, CoinSpend, Program};

    use super::*;

    #[test]
    fn normalization_progress_detects_cycles_without_false_identity_matches() {
        let coin = Coin::new([1; 32].into(), [2; 32].into(), 1);
        let first = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                Program::from(vec![1]),
                Program::from(vec![2]),
            )],
            Signature::default(),
        );
        let second = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                Program::from(vec![1]),
                Program::from(vec![3]),
            )],
            Signature::default(),
        );
        let mut progress = NormalizationProgress::default();

        assert!(progress.record(&first));
        assert!(progress.record(&second));
        assert!(!progress.record(&first));
    }
}
