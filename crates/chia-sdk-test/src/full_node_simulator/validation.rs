use chia_consensus::{
    conditions::ELIGIBLE_FOR_DEDUP, flags::COMPUTE_FINGERPRINT, validation_error::ErrorCode,
};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_sdk_types::default_constants;
use clvmr::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
use indexmap::{IndexMap, IndexSet};

use crate::{
    FullNodeSimulator, SimulatorError,
    full_node_simulator::{SIMULATOR_GENESIS_CHALLENGE, ValidatedBundle, ValidatedSpend},
    spend_bundle_validation::{
        CoinRecord, ValidationClock, ValidationSettings, validate_conditions,
        validate_relative_conditions, validate_reserve_fee,
    },
};

#[derive(Debug, Default)]
pub(super) struct ValidationOverlay {
    additions: IndexSet<Bytes32>,
    removals: IndexSet<Bytes32>,
}

impl ValidationOverlay {
    #[cfg(feature = "serde")]
    pub(super) fn apply(&mut self, bundle: &ValidatedBundle) {
        self.additions
            .extend(bundle.additions.iter().map(|(coin, _)| coin.coin_id()));
        self.removals.extend(bundle.removals.iter().copied());
    }
}

impl FullNodeSimulator {
    pub(super) fn validate_bundle(
        &self,
        spend_bundle: SpendBundle,
    ) -> Result<ValidatedBundle, SimulatorError> {
        self.validate_bundle_with_overlay(spend_bundle, None)
    }

    #[cfg(feature = "serde")]
    pub(super) fn validate_bundle_in_block(
        &self,
        spend_bundle: SpendBundle,
        overlay: &ValidationOverlay,
    ) -> Result<ValidatedBundle, SimulatorError> {
        self.validate_bundle_with_overlay(spend_bundle, Some(overlay))
    }

    fn validate_bundle_with_overlay(
        &self,
        spend_bundle: SpendBundle,
        overlay: Option<&ValidationOverlay>,
    ) -> Result<ValidatedBundle, SimulatorError> {
        let constants = default_constants(SIMULATOR_GENESIS_CHALLENGE, SIMULATOR_GENESIS_CHALLENGE);
        let clock = ValidationClock {
            height: self.state.height,
            timestamp: self.state.next_timestamp,
        };
        let validation = validate_conditions(
            &spend_bundle,
            ValidationSettings {
                constants: &constants,
                max_cost: 11_000_000_000 / 2,
                flags: ENABLE_KECCAK_OPS_OUTSIDE_GUARD | COMPUTE_FINGERPRINT,
                clock,
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

        for spend in &conds.spends {
            let coin_id = spend.coin_id;
            if !removals.insert(coin_id) {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }
            if overlay.is_some_and(|overlay| overlay.removals.contains(&coin_id)) {
                return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
            }

            if let Some(record) = self.state.coins.get(&coin_id) {
                if record.spent_block_index.is_some() {
                    return Err(SimulatorError::Validation(ErrorCode::DoubleSpend));
                }

                validate_relative_conditions(
                    spend,
                    CoinRecord {
                        created_height: Some(record.confirmed_block_index),
                        created_timestamp: Some(record.timestamp),
                    },
                    clock,
                )
                .map_err(SimulatorError::Validation)?;
            } else if additions.contains_key(&coin_id)
                || overlay.is_some_and(|overlay| overlay.additions.contains(&coin_id))
                || self.mempool_addition_coin(coin_id).is_some()
            {
                validate_relative_conditions(
                    spend,
                    CoinRecord {
                        created_height: Some(clock.height),
                        created_timestamp: Some(clock.timestamp),
                    },
                    clock,
                )
                .map_err(SimulatorError::Validation)?;
            } else {
                return Err(SimulatorError::Validation(ErrorCode::UnknownUnspent));
            }
        }

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

    fn mempool_addition_coin(&self, coin_id: Bytes32) -> Option<Coin> {
        self.mempool
            .values()
            .flat_map(|item| item.additions.iter().map(|(coin, _)| *coin))
            .find(|coin| coin.coin_id() == coin_id)
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
                    AssertBeforeSecondsAbsolute::new(sim.state.next_timestamp).into()
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
