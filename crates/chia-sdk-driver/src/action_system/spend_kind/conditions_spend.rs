use std::collections::HashSet;

use chia_sdk_types::Conditions;

use crate::{DriverError, Output};

#[derive(Debug, Default, Clone)]
pub struct ConditionsSpend {
    conditions: Conditions,
    outputs: HashSet<Output>,
}

impl ConditionsSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_output(&self, output: &Output) -> bool {
        self.outputs.contains(output)
    }

    pub fn add_conditions(&mut self, conditions: Conditions) -> Result<(), DriverError> {
        // Check for duplicate outputs first to avoid inserting conditions that should be rejected
        for condition in &conditions {
            if let Some(create_coin) = condition.as_create_coin() {
                if self.has_output(&Output::new(create_coin.puzzle_hash, create_coin.amount)) {
                    return Err(DriverError::DuplicateOutput);
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
