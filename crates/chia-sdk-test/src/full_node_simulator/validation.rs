use std::collections::HashMap;

use chia_consensus::{
    conditions::ELIGIBLE_FOR_DEDUP,
    consensus_constants::ConsensusConstants,
    flags::{ConsensusFlags, MEMPOOL_MODE},
    validation_error::ErrorCode,
};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_sdk_types::default_constants;
use indexmap::{IndexMap, IndexSet};

use crate::{
    FullNodeSimulator, SimulatorError,
    full_node_simulator::{SIMULATOR_GENESIS_CHALLENGE, ValidatedBundle, ValidatedSpend},
    spend_bundle_validation::{
        ValidationClock, ValidationSettings, coin_record, ephemeral_coin_record,
        has_relative_time_lock, validate_conditions, validate_reserve_fee, validate_time_locks,
    },
};

#[derive(Debug, Default)]
pub(super) struct ValidationOverlay {
    additions: IndexMap<Bytes32, Coin>,
    removals: IndexSet<Bytes32>,
}

impl ValidationOverlay {
    #[cfg(feature = "serde")]
    pub(super) fn apply(&mut self, bundle: &ValidatedBundle) {
        self.apply_additions(bundle);
        self.removals.extend(bundle.removals.iter().copied());
    }

    /// Makes the coins created by a bundle spendable by later bundles in the same block. Removals
    /// aren't tracked, so the caller can allow identical spends to be deduplicated.
    pub(super) fn apply_additions(&mut self, bundle: &ValidatedBundle) {
        self.additions.extend(
            bundle
                .additions
                .iter()
                .map(|(coin, _)| (coin.coin_id(), *coin)),
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum ValidationContext<'a> {
    /// A transaction submitted to the mempool, which may spend coins created by other mempool
    /// items. Mempool rules apply unless mempool mode is disabled.
    Mempool,
    /// A transaction in a block, which may only spend coins that are confirmed or created earlier
    /// in the block. Mempool rules don't apply, just as when mainnet validates a block.
    Block(&'a ValidationOverlay),
}

impl FullNodeSimulator {
    pub(super) fn constants() -> ConsensusConstants {
        default_constants(SIMULATOR_GENESIS_CHALLENGE, SIMULATOR_GENESIS_CHALLENGE)
    }

    /// Validates a transaction that's being submitted to the mempool.
    pub(super) fn validate_bundle(
        &self,
        spend_bundle: SpendBundle,
    ) -> Result<ValidatedBundle, SimulatorError> {
        self.validate_bundle_in_context(spend_bundle, ValidationContext::Mempool)
    }

    /// Validates a transaction that's being included in a block, after the transactions that
    /// were applied to the overlay.
    pub(super) fn validate_bundle_in_block(
        &self,
        spend_bundle: SpendBundle,
        overlay: &ValidationOverlay,
    ) -> Result<ValidatedBundle, SimulatorError> {
        self.validate_bundle_in_context(spend_bundle, ValidationContext::Block(overlay))
    }

    fn validate_bundle_in_context(
        &self,
        spend_bundle: SpendBundle,
        context: ValidationContext<'_>,
    ) -> Result<ValidatedBundle, SimulatorError> {
        let constants = Self::constants();
        let clock = ValidationClock {
            height: self.state.height,
            timestamp: self.state.peak_timestamp(),
        };
        let mut flags =
            ConsensusFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD | ConsensusFlags::COMPUTE_FINGERPRINT;
        if self.mempool_mode && matches!(context, ValidationContext::Mempool) {
            flags |= MEMPOOL_MODE;
        }
        let validation = validate_conditions(
            &spend_bundle,
            ValidationSettings {
                constants: &constants,
                max_cost: 11_000_000_000 / 2,
                flags,
            },
        )
        .map_err(SimulatorError::Validation)?;
        let conds = validation.conditions;

        let bundle_coin_spends = spend_bundle
            .coin_spends
            .iter()
            .map(|spend| (spend.coin.coin_id(), spend.clone()))
            .collect::<IndexMap<_, _>>();

        let mut removals = IndexSet::new();
        let mut additions = IndexMap::new();
        let mut spends = IndexMap::new();

        for (spend, parsed) in conds.spends.iter().zip(validation.additions) {
            let coin_id = spend.coin_id;
            debug_assert_eq!(parsed.coin_id, coin_id);
            let spend_additions = parsed
                .additions
                .into_iter()
                .map(|addition| (addition.coin, addition.hint))
                .collect::<Vec<_>>();
            for (coin, hint) in &spend_additions {
                additions.insert(coin.coin_id(), (*coin, *hint));
            }

            let Some(coin_spend) = bundle_coin_spends.get(&coin_id).cloned() else {
                return Err(SimulatorError::Validation(ErrorCode::InvalidSpendBundle));
            };

            let fingerprint = if (spend.flags & ELIGIBLE_FOR_DEDUP) != 0 {
                Bytes32::try_from(spend.fingerprint.as_ref()).ok()
            } else {
                None
            };

            spends.insert(
                coin_id,
                ValidatedSpend {
                    coin_spend,
                    flags: spend.flags,
                    fingerprint,
                    additions: spend_additions,
                },
            );
        }

        let mut removal_records = HashMap::new();

        for spend in &conds.spends {
            let coin_id = spend.coin_id;
            if !removals.insert(coin_id) {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }
            if let ValidationContext::Block(overlay) = context
                && overlay.removals.contains(&coin_id)
            {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            let record = if let Some(record) = self.state.coins.get(&coin_id) {
                if record.spent_block_index.is_some() {
                    return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
                }
                coin_record(
                    record.coin,
                    record.confirmed_block_index,
                    record.coinbase,
                    record.timestamp,
                )
            } else if let Some((coin, _)) = additions.get(&coin_id) {
                ephemeral_coin_record(*coin, clock)
            } else if let Some(coin) = self.unconfirmed_addition(coin_id, context) {
                // The coin is created by another transaction in the same block, so consensus
                // treats it as ephemeral. Condition parsing can't detect this across bundles.
                if has_relative_time_lock(spend) {
                    return Err(SimulatorError::Validation(
                        ErrorCode::EphemeralRelativeCondition,
                    ));
                }
                ephemeral_coin_record(coin, clock)
            } else {
                return Err(SimulatorError::Validation(ErrorCode::UnknownUnspent));
            };
            removal_records.insert(coin_id, record);
        }

        validate_time_locks(&conds, &removal_records, clock).map_err(SimulatorError::Validation)?;
        validate_reserve_fee(&conds, validation.fee).map_err(SimulatorError::Validation)?;

        Ok(ValidatedBundle {
            spend_bundle,
            removals: removals.into_iter().collect(),
            additions: additions.into_values().collect(),
            spends,
            cost: conds.cost,
            fee: validation.fee,
        })
    }

    fn unconfirmed_addition(
        &self,
        coin_id: Bytes32,
        context: ValidationContext<'_>,
    ) -> Option<Coin> {
        match context {
            ValidationContext::Mempool => self
                .mempool
                .values()
                .flat_map(|item| item.additions.iter().map(|(coin, _)| *coin))
                .find(|coin| coin.coin_id() == coin_id),
            ValidationContext::Block(overlay) => overlay.additions.get(&coin_id).copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_consensus::validation_error::ErrorCode;
    use chia_protocol::{Coin, CoinSpend, SpendBundle};
    use chia_sdk_types::conditions::{
        AssertBeforeHeightAbsolute, AssertBeforeHeightRelative, AssertBeforeSecondsAbsolute,
        AssertBeforeSecondsRelative, Condition, Conditions, CreateCoin, Memos,
    };
    use clvmr::NodePtr;

    use crate::{FullNodeSimulator, SimulatorError, to_program, to_puzzle};

    fn spend_with_condition(
        coin: Coin,
        puzzle_reveal: chia_protocol::Program,
        puzzle_hash: chia_protocol::Bytes32,
        condition: Condition<NodePtr>,
    ) -> anyhow::Result<SpendBundle> {
        let conditions = Conditions::new().with(condition).with(CreateCoin::new(
            puzzle_hash,
            coin.amount - 1,
            Memos::None,
        ));
        Ok(SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(conditions)?)],
            Signature::default(),
        ))
    }

    #[test]
    fn assert_before_conditions_reject_equality() -> anyhow::Result<()> {
        let cases = [
            (
                AssertBeforeHeightAbsolute::new(1).into(),
                ErrorCode::AssertBeforeHeightAbsoluteFailed,
            ),
            (
                AssertBeforeSecondsAbsolute::new(1).into(),
                ErrorCode::AssertBeforeSecondsAbsoluteFailed,
            ),
            (
                AssertBeforeHeightRelative::new(0).into(),
                ErrorCode::AssertBeforeHeightRelativeFailed,
            ),
            (
                AssertBeforeSecondsRelative::new(0).into(),
                ErrorCode::AssertBeforeSecondsRelativeFailed,
            ),
        ];

        for (condition, expected_error) in cases {
            let mut sim = FullNodeSimulator::new();
            let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
            let coin = sim.new_coin(puzzle_hash, 100);
            let condition = match expected_error {
                ErrorCode::AssertBeforeSecondsAbsoluteFailed => {
                    AssertBeforeSecondsAbsolute::new(sim.state.peak_timestamp()).into()
                }
                _ => condition,
            };
            let spend_bundle = spend_with_condition(coin, puzzle_reveal, puzzle_hash, condition)?;

            assert!(matches!(
                sim.validate_bundle(spend_bundle),
                Err(SimulatorError::Validation(error)) if error == expected_error
            ));
        }

        Ok(())
    }
}
