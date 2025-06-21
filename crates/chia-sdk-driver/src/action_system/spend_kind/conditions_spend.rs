use std::collections::HashSet;

use chia_sdk_types::{Condition, Conditions};

use crate::{Output, OutputSet};

#[derive(Debug, Default, Clone)]
pub struct ConditionsSpend {
    conditions: Conditions,
    outputs: HashSet<Output>,
}

impl ConditionsSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_conditions(&mut self, conditions: Conditions) {
        for condition in conditions {
            if let Some(create_coin) = condition.as_create_coin() {
                let output = Output::new(create_coin.puzzle_hash, create_coin.amount);
                self.outputs.insert(output);
            }
            self.conditions.push(condition);
        }
    }

    pub fn finish(self) -> Conditions {
        self.conditions
    }
}

impl OutputSet for ConditionsSpend {
    fn has_output(&self, output: &Output) -> bool {
        self.outputs.contains(output)
    }

    fn can_run_cat_tail(&self) -> bool {
        !self.conditions.iter().any(Condition::is_run_cat_tail)
    }

    fn missing_singleton_output(&self) -> bool {
        !self.conditions.iter().any(|condition| {
            condition.is_melt_singleton()
                || condition
                    .as_create_coin()
                    .is_some_and(|create_coin| create_coin.amount % 2 == 1)
        })
    }
}
