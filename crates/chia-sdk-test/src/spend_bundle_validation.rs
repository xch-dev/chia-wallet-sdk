use std::collections::{HashMap, HashSet};

use chia_consensus::{
    check_time_locks::check_time_locks,
    consensus_constants::ConsensusConstants,
    owned_conditions::{OwnedSpendBundleConditions, OwnedSpendConditions},
    validation_error::ErrorCode,
};
use chia_protocol::{Bytes32, Coin, CoinRecord, SpendBundle};

use crate::validate_clvm_and_signature;

/// The peak (most recent transaction block) that a spend bundle is validated against.
///
/// Like mainnet, time locks are checked against the previous transaction block rather than the
/// block the spend bundle will be included in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValidationClock {
    pub height: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CreatedCoin {
    pub coin: Coin,
    pub hint: Option<Bytes32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpendAdditions {
    pub coin_id: Bytes32,
    pub additions: Vec<CreatedCoin>,
}

#[derive(Debug)]
pub(crate) struct ValidatedConditions {
    pub conditions: OwnedSpendBundleConditions,
    pub additions: Vec<SpendAdditions>,
    pub fee: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ValidationSettings<'a> {
    pub constants: &'a ConsensusConstants,
    pub max_cost: u64,
    pub flags: u32,
}

pub(crate) fn validate_conditions(
    spend_bundle: &SpendBundle,
    settings: ValidationSettings<'_>,
) -> Result<ValidatedConditions, ErrorCode> {
    if spend_bundle.coin_spends.is_empty() {
        return Err(ErrorCode::InvalidSpendBundle);
    }

    let conditions = validate_clvm_and_signature(
        spend_bundle,
        settings.max_cost,
        settings.constants,
        settings.flags,
    )?;

    validate_puzzle_hashes(spend_bundle, &conditions)?;

    let additions = conditions
        .spends
        .iter()
        .map(parse_additions)
        .collect::<Vec<_>>();
    let fee = conditions
        .removal_amount
        .saturating_sub(conditions.addition_amount)
        .try_into()
        .unwrap_or(u64::MAX);

    Ok(ValidatedConditions {
        conditions,
        additions,
        fee,
    })
}

/// Checks absolute, relative, and birth time locks with the same rules as mainnet.
///
/// Every removal must have a coin record. Coins created in the same spend bundle should use
/// [`ephemeral_coin_record`].
pub(crate) fn validate_time_locks(
    conditions: &OwnedSpendBundleConditions,
    removal_records: &HashMap<Bytes32, CoinRecord>,
    clock: ValidationClock,
) -> Result<(), ErrorCode> {
    check_time_locks(removal_records, conditions, clock.height, clock.timestamp)
}

pub(crate) fn coin_record(
    coin: Coin,
    confirmed_block_index: u32,
    coinbase: bool,
    timestamp: u64,
) -> CoinRecord {
    CoinRecord::new(coin, confirmed_block_index, 0, coinbase, timestamp)
}

/// Mirrors the record the mainnet mempool synthesizes for a coin created and spent in the same
/// spend bundle. Relative and birth conditions on such coins are already rejected while parsing
/// conditions, so these values only need to be consistent.
pub(crate) fn ephemeral_coin_record(coin: Coin, clock: ValidationClock) -> CoinRecord {
    coin_record(coin, clock.height + 1, false, clock.timestamp)
}

/// Whether a spend has a condition which consensus doesn't allow on a coin that is created in
/// the same block it's spent in.
pub(crate) fn has_relative_time_lock(spend: &OwnedSpendConditions) -> bool {
    spend.height_relative.is_some()
        || spend.seconds_relative.is_some()
        || spend.before_height_relative.is_some()
        || spend.before_seconds_relative.is_some()
        || spend.birth_height.is_some()
        || spend.birth_seconds.is_some()
}

pub(crate) fn validate_reserve_fee(
    conditions: &OwnedSpendBundleConditions,
    fee: u64,
) -> Result<(), ErrorCode> {
    if fee < conditions.reserve_fee {
        return Err(ErrorCode::ReserveFeeConditionFailed);
    }
    Ok(())
}

fn validate_puzzle_hashes(
    spend_bundle: &SpendBundle,
    conditions: &OwnedSpendBundleConditions,
) -> Result<(), ErrorCode> {
    let bundle_puzzle_hashes = spend_bundle
        .coin_spends
        .iter()
        .map(|spend| spend.coin.puzzle_hash)
        .collect::<HashSet<_>>();
    let condition_puzzle_hashes = conditions
        .spends
        .iter()
        .map(|spend| spend.puzzle_hash)
        .collect::<HashSet<_>>();

    if bundle_puzzle_hashes != condition_puzzle_hashes {
        return Err(ErrorCode::InvalidSpendBundle);
    }
    Ok(())
}

fn parse_additions(spend: &OwnedSpendConditions) -> SpendAdditions {
    let additions = spend
        .create_coin
        .iter()
        .map(|(puzzle_hash, amount, hint)| CreatedCoin {
            coin: Coin::new(spend.coin_id, *puzzle_hash, *amount),
            hint: hint
                .as_ref()
                .filter(|bytes| bytes.len() == 32)
                .and_then(|bytes| Bytes32::try_from(bytes.as_ref()).ok()),
        })
        .collect();

    SpendAdditions {
        coin_id: spend.coin_id,
        additions,
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_consensus::owned_conditions::{OwnedSpendBundleConditions, OwnedSpendConditions};
    use chia_consensus::validation_error::ErrorCode;
    use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program, SpendBundle};
    use chia_sdk_types::conditions::{
        AssertBeforeHeightAbsolute, AssertHeightAbsolute, AssertHeightRelative,
        AssertMyBirthHeight, AssertMyBirthSeconds, AssertSecondsAbsolute, AssertSecondsRelative,
        Condition, Conditions, CreateCoin, Memos,
    };
    use clvmr::NodePtr;
    use indexmap::indexset;

    use std::collections::HashMap;

    use super::{
        ValidationClock, coin_record, parse_additions, validate_reserve_fee, validate_time_locks,
    };
    use crate::{
        FullNodeSimulator, Simulator, SimulatorConfig, SimulatorError, TimestampMode, to_program,
        to_puzzle,
    };

    fn spend_with_condition(
        coin: Coin,
        puzzle_reveal: Program,
        puzzle_hash: Bytes32,
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
    fn simulators_match_on_shared_absolute_validation() -> anyhow::Result<()> {
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let mut legacy = Simulator::new();
        legacy.create_block();
        let legacy_coin = legacy.new_coin(puzzle_hash, 100);
        let legacy_bundle = spend_with_condition(
            legacy_coin,
            puzzle_reveal.clone(),
            puzzle_hash,
            AssertHeightAbsolute::new(legacy.height() + 1).into(),
        )?;
        assert!(matches!(
            legacy.new_transaction(legacy_bundle),
            Err(SimulatorError::Validation(
                ErrorCode::AssertHeightAbsoluteFailed
            ))
        ));

        let mut full_node = FullNodeSimulator::new();
        let full_node_coin = full_node.new_coin(puzzle_hash, 100);
        let full_node_bundle = spend_with_condition(
            full_node_coin,
            puzzle_reveal,
            puzzle_hash,
            AssertHeightAbsolute::new(full_node.height() + 1).into(),
        )?;
        assert!(matches!(
            full_node.push_tx_detailed(full_node_bundle).error,
            Some(SimulatorError::Validation(
                ErrorCode::AssertHeightAbsoluteFailed
            ))
        ));

        Ok(())
    }

    #[test]
    fn simulators_reject_assert_before_equality() -> anyhow::Result<()> {
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let mut legacy = Simulator::new();
        legacy.create_block();
        let legacy_coin = legacy.new_coin(puzzle_hash, 100);
        let legacy_bundle = spend_with_condition(
            legacy_coin,
            puzzle_reveal.clone(),
            puzzle_hash,
            AssertBeforeHeightAbsolute::new(legacy.height()).into(),
        )?;
        assert!(matches!(
            legacy.new_transaction(legacy_bundle),
            Err(SimulatorError::Validation(
                ErrorCode::AssertBeforeHeightAbsoluteFailed
            ))
        ));

        let mut full_node = FullNodeSimulator::new();
        let full_node_coin = full_node.new_coin(puzzle_hash, 100);
        let full_node_bundle = spend_with_condition(
            full_node_coin,
            puzzle_reveal,
            puzzle_hash,
            AssertBeforeHeightAbsolute::new(full_node.height()).into(),
        )?;
        assert!(matches!(
            full_node.push_tx_detailed(full_node_bundle).error,
            Some(SimulatorError::Validation(
                ErrorCode::AssertBeforeHeightAbsoluteFailed
            ))
        ));

        Ok(())
    }

    fn assert_validation_error<T: std::fmt::Debug>(
        result: Result<T, SimulatorError>,
        expected: ErrorCode,
    ) {
        match result {
            Err(SimulatorError::Validation(error)) => assert_eq!(error, expected),
            other => panic!("expected {expected:?}, got {other:?}"),
        }
    }

    #[test]
    fn simulators_check_birth_assertions() -> anyhow::Result<()> {
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let mut legacy = Simulator::new();
        legacy.create_block();
        let legacy_coin = legacy.new_coin(puzzle_hash, 100);
        let (height, timestamp) = (legacy.height(), legacy.timestamp());
        let cases: [(Condition<NodePtr>, ErrorCode); 2] = [
            (
                AssertMyBirthHeight::new(height + 1).into(),
                ErrorCode::AssertMyBirthHeightFailed,
            ),
            (
                AssertMyBirthSeconds::new(timestamp + 1).into(),
                ErrorCode::AssertMyBirthSecondsFailed,
            ),
        ];
        for (condition, expected) in cases {
            let bundle =
                spend_with_condition(legacy_coin, puzzle_reveal.clone(), puzzle_hash, condition)?;
            assert_validation_error(legacy.new_transaction(bundle), expected);
        }
        let bundle = spend_with_condition(
            legacy_coin,
            puzzle_reveal.clone(),
            puzzle_hash,
            AssertMyBirthSeconds::new(timestamp).into(),
        )?;
        legacy.new_transaction(bundle)?;

        let mut full_node = FullNodeSimulator::new();
        let full_node_coin = full_node.new_coin(puzzle_hash, 100);
        let record = full_node
            .get_coin_record_by_name(full_node_coin.coin_id())
            .coin_record
            .expect("missing coin record");
        let cases: [(Condition<NodePtr>, ErrorCode); 2] = [
            (
                AssertMyBirthHeight::new(record.confirmed_block_index + 1).into(),
                ErrorCode::AssertMyBirthHeightFailed,
            ),
            (
                AssertMyBirthSeconds::new(record.timestamp + 1).into(),
                ErrorCode::AssertMyBirthSecondsFailed,
            ),
        ];
        for (condition, expected) in cases {
            let bundle = spend_with_condition(
                full_node_coin,
                puzzle_reveal.clone(),
                puzzle_hash,
                condition,
            )?;
            assert_validation_error(
                full_node.push_tx_detailed(bundle).error.map_or(Ok(()), Err),
                expected,
            );
        }
        let bundle = spend_with_condition(
            full_node_coin,
            puzzle_reveal,
            puzzle_hash,
            AssertMyBirthHeight::new(record.confirmed_block_index).into(),
        )?;
        assert!(full_node.push_tx(bundle).success);

        Ok(())
    }

    #[test]
    fn legacy_simulator_validates_time_locks_against_peak() -> anyhow::Result<()> {
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        let cases: [(Condition<NodePtr>, ErrorCode); 2] = [
            (
                AssertHeightRelative::new(1).into(),
                ErrorCode::AssertHeightRelativeFailed,
            ),
            (
                AssertSecondsRelative::new(1).into(),
                ErrorCode::AssertSecondsRelativeFailed,
            ),
        ];

        for (condition, expected) in cases {
            let mut sim = Simulator::new();
            let coin = sim.new_coin(puzzle_hash, 100);
            let bundle = spend_with_condition(
                coin,
                puzzle_reveal.clone(),
                puzzle_hash,
                AssertHeightAbsolute::new(0).into(),
            )?;
            sim.new_transaction(bundle)?;

            // The child is confirmed in the peak block, so the lock can't pass until the next one.
            let child = Coin::new(coin.coin_id(), puzzle_hash, coin.amount - 1);
            let bundle =
                spend_with_condition(child, puzzle_reveal.clone(), puzzle_hash, condition)?;
            assert_validation_error(sim.new_transaction(bundle.clone()), expected);

            sim.create_block();
            sim.new_transaction(bundle)?;
        }

        let mut sim = Simulator::new();
        let coin = sim.new_coin(puzzle_hash, 100);
        sim.set_next_timestamp(100)?;
        let bundle = spend_with_condition(
            coin,
            puzzle_reveal,
            puzzle_hash,
            AssertSecondsAbsolute::new(100).into(),
        )?;
        assert_validation_error(
            sim.new_transaction(bundle.clone()),
            ErrorCode::AssertSecondsAbsoluteFailed,
        );

        sim.create_block();
        assert_eq!(sim.timestamp(), 100);
        sim.new_transaction(bundle)?;

        Ok(())
    }

    #[test]
    fn simulators_use_mempool_mode_unless_disabled() -> anyhow::Result<()> {
        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
        // Unknown condition opcodes are only rejected in mempool mode.
        let solution = to_program([(200_u16, ())])?;

        for mempool_mode in [true, false] {
            let mut legacy = Simulator::with_config(SimulatorConfig {
                mempool_mode,
                ..Default::default()
            });
            let coin = legacy.new_coin(puzzle_hash, 1);
            let bundle = SpendBundle::new(
                vec![CoinSpend::new(
                    coin,
                    puzzle_reveal.clone(),
                    solution.clone(),
                )],
                Signature::default(),
            );
            assert_eq!(legacy.new_transaction(bundle).is_ok(), !mempool_mode);

            let mut full_node = FullNodeSimulator::new();
            full_node.set_mempool_mode(mempool_mode);
            let coin = full_node.new_coin(puzzle_hash, 1);
            let bundle = SpendBundle::new(
                vec![CoinSpend::new(
                    coin,
                    puzzle_reveal.clone(),
                    solution.clone(),
                )],
                Signature::default(),
            );
            assert_eq!(full_node.push_tx(bundle).success, !mempool_mode);
        }

        Ok(())
    }

    #[test]
    fn legacy_simulator_uses_timestamp_mode() -> anyhow::Result<()> {
        let mut sim = Simulator::with_config(SimulatorConfig {
            timestamp_mode: TimestampMode::Increment {
                start: 1000,
                step: 10,
            },
            ..Default::default()
        });
        assert_eq!(sim.timestamp(), 1000);
        assert_eq!(sim.next_timestamp(), 1010);
        sim.create_block();
        assert_eq!(sim.timestamp(), 1010);
        assert_eq!(sim.next_timestamp(), 1020);

        let mut sim = Simulator::with_config(SimulatorConfig {
            timestamp_mode: TimestampMode::Fixed(500),
            ..Default::default()
        });
        sim.create_block();
        assert_eq!(sim.timestamp(), 500);
        sim.pass_time(5);
        sim.create_block();
        assert_eq!(sim.timestamp(), 505);
        assert_eq!(sim.next_timestamp(), 505);
        assert!(sim.set_next_timestamp(504).is_err());

        Ok(())
    }

    #[test]
    fn lookup_puzzle_hashes_ignores_hints_for_unknown_coins() {
        let mut sim = Simulator::new();
        let hint = Bytes32::new([1; 32]);
        sim.hint_coin(Bytes32::new([2; 32]), hint);
        assert!(sim.lookup_puzzle_hashes(indexset![hint], true).is_empty());
    }

    #[test]
    fn parses_valid_hints_and_ignores_other_memos() {
        let valid_hint = Bytes32::from([7; 32]);
        let spend = OwnedSpendConditions {
            coin_id: Bytes32::from([1; 32]),
            create_coin: vec![
                (Bytes32::from([2; 32]), 3, Some(valid_hint.to_vec().into())),
                (Bytes32::from([4; 32]), 5, Some(Bytes::from(vec![6; 31]))),
                (Bytes32::from([7; 32]), 8, None),
            ],
            ..Default::default()
        };

        let parsed = parse_additions(&spend);

        assert_eq!(parsed.additions.len(), 3);
        assert_eq!(parsed.additions[0].hint, Some(valid_hint));
        assert_eq!(parsed.additions[1].hint, None);
        assert_eq!(parsed.additions[2].hint, None);
    }

    #[test]
    fn absolute_and_relative_before_checks_reject_equality() {
        let clock = ValidationClock {
            height: 10,
            timestamp: 20,
        };
        let coin_id = Bytes32::from([1; 32]);
        let records = HashMap::from([(
            coin_id,
            coin_record(
                Coin::new(Bytes32::default(), Bytes32::default(), 1),
                clock.height,
                false,
                clock.timestamp,
            ),
        )]);

        let absolute = OwnedSpendBundleConditions {
            before_height_absolute: Some(clock.height),
            before_seconds_absolute: Some(clock.timestamp),
            ..Default::default()
        };
        assert_eq!(
            validate_time_locks(&absolute, &records, clock),
            Err(ErrorCode::AssertBeforeHeightAbsoluteFailed)
        );

        let relative = OwnedSpendBundleConditions {
            spends: vec![OwnedSpendConditions {
                coin_id,
                before_height_relative: Some(0),
                before_seconds_relative: Some(0),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            validate_time_locks(&relative, &records, clock),
            Err(ErrorCode::AssertBeforeHeightRelativeFailed)
        );
    }

    #[test]
    fn reserve_fee_uses_saturated_bundle_fee() {
        let conditions = OwnedSpendBundleConditions {
            reserve_fee: 6,
            removal_amount: 10,
            addition_amount: 5,
            ..Default::default()
        };

        assert_eq!(
            validate_reserve_fee(&conditions, 5),
            Err(ErrorCode::ReserveFeeConditionFailed)
        );
        assert!(validate_reserve_fee(&conditions, 6).is_ok());
    }
}
