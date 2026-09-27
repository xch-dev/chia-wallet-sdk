use std::collections::HashSet;

use chia_consensus::{
    consensus_constants::ConsensusConstants,
    owned_conditions::{OwnedSpendBundleConditions, OwnedSpendConditions},
    validation_error::ErrorCode,
};
use chia_protocol::{Bytes32, Coin, SpendBundle};

use crate::validate_clvm_and_signature;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValidationClock {
    pub height: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoinRecord {
    pub created_height: Option<u32>,
    pub created_timestamp: Option<u64>,
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
    pub clock: ValidationClock,
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
    validate_absolute_conditions(&conditions, settings.clock)?;

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

pub(crate) fn validate_relative_conditions(
    spend: &OwnedSpendConditions,
    record: CoinRecord,
    clock: ValidationClock,
) -> Result<(), ErrorCode> {
    if let Some(relative_height) = spend.height_relative {
        let created_height = record
            .created_height
            .ok_or(ErrorCode::EphemeralRelativeCondition)?;
        if clock.height < created_height + relative_height {
            return Err(ErrorCode::AssertHeightRelativeFailed);
        }
    }

    if let Some(relative_seconds) = spend.seconds_relative {
        let created_timestamp = record
            .created_timestamp
            .ok_or(ErrorCode::EphemeralRelativeCondition)?;
        if clock.timestamp < created_timestamp + relative_seconds {
            return Err(ErrorCode::AssertSecondsRelativeFailed);
        }
    }

    if let Some(relative_height) = spend.before_height_relative {
        let created_height = record
            .created_height
            .ok_or(ErrorCode::EphemeralRelativeCondition)?;
        if created_height + relative_height <= clock.height {
            return Err(ErrorCode::AssertBeforeHeightRelativeFailed);
        }
    }

    if let Some(relative_seconds) = spend.before_seconds_relative {
        let created_timestamp = record
            .created_timestamp
            .ok_or(ErrorCode::EphemeralRelativeCondition)?;
        if created_timestamp + relative_seconds <= clock.timestamp {
            return Err(ErrorCode::AssertBeforeSecondsRelativeFailed);
        }
    }

    Ok(())
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

fn validate_absolute_conditions(
    conditions: &OwnedSpendBundleConditions,
    clock: ValidationClock,
) -> Result<(), ErrorCode> {
    if clock.height < conditions.height_absolute {
        return Err(ErrorCode::AssertHeightAbsoluteFailed);
    }
    if clock.timestamp < conditions.seconds_absolute {
        return Err(ErrorCode::AssertSecondsAbsoluteFailed);
    }
    if let Some(height) = conditions.before_height_absolute
        && height <= clock.height
    {
        return Err(ErrorCode::AssertBeforeHeightAbsoluteFailed);
    }
    if let Some(timestamp) = conditions.before_seconds_absolute
        && timestamp <= clock.timestamp
    {
        return Err(ErrorCode::AssertBeforeSecondsAbsoluteFailed);
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
        AssertBeforeHeightAbsolute, AssertHeightAbsolute, Condition, Conditions, CreateCoin, Memos,
    };
    use clvmr::NodePtr;

    use super::{
        CoinRecord, ValidationClock, parse_additions, validate_absolute_conditions,
        validate_relative_conditions, validate_reserve_fee,
    };
    use crate::{FullNodeSimulator, Simulator, SimulatorError, to_program, to_puzzle};

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
        let conditions = OwnedSpendBundleConditions {
            before_height_absolute: Some(clock.height),
            before_seconds_absolute: Some(clock.timestamp),
            ..Default::default()
        };
        let spend = OwnedSpendConditions {
            before_height_relative: Some(0),
            before_seconds_relative: Some(0),
            ..Default::default()
        };
        let record = CoinRecord {
            created_height: Some(clock.height),
            created_timestamp: Some(clock.timestamp),
        };

        assert_eq!(
            validate_absolute_conditions(&conditions, clock),
            Err(ErrorCode::AssertBeforeHeightAbsoluteFailed)
        );
        assert_eq!(
            validate_relative_conditions(&spend, record, clock),
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
