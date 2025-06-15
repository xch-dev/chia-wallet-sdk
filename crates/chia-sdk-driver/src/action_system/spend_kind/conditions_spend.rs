use chia_sdk_types::Conditions;

use crate::{DriverError, Output, OutputConstraint, OutputSet};

#[derive(Debug, Clone)]
pub struct ConditionsSpend {
    conditions: Conditions,
    outputs: OutputSet,
}

impl ConditionsSpend {
    pub fn new(constraints: Vec<OutputConstraint>) -> Self {
        Self {
            conditions: Conditions::new(),
            outputs: OutputSet::new(constraints),
        }
    }

    pub fn outputs(&self) -> &OutputSet {
        &self.outputs
    }

    pub fn add_conditions(&mut self, conditions: Conditions) -> Result<(), DriverError> {
        // Check for duplicate outputs first to avoid inserting conditions that should be rejected
        for condition in &conditions {
            if let Some(create_coin) = condition.as_create_coin() {
                if !self
                    .outputs
                    .is_allowed(&Output::new(create_coin.puzzle_hash, create_coin.amount))
                {
                    return Err(DriverError::InvalidOutput);
                }
            }
        }

        for condition in conditions {
            if let Some(&create_coin) = condition.as_create_coin() {
                self.outputs
                    .insert(Output::new(create_coin.puzzle_hash, create_coin.amount));
            }
            self.conditions.push(condition);
        }

        Ok(())
    }

    pub fn finish(self) -> Conditions {
        self.conditions
    }
}
