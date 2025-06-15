mod conditions_spend;

pub use conditions_spend::*;

use crate::{OutputConstraint, OutputSet};

#[derive(Debug, Clone)]
pub enum SpendKind {
    Conditions(ConditionsSpend),
}

impl SpendKind {
    pub fn conditions(constraints: Vec<OutputConstraint>) -> Self {
        Self::Conditions(ConditionsSpend::new(constraints))
    }

    pub fn outputs(&self) -> &OutputSet {
        match self {
            Self::Conditions(spend) => spend.outputs(),
        }
    }

    #[must_use]
    pub fn child(&self) -> Self {
        match self {
            Self::Conditions(spend) => Self::conditions(spend.outputs().constraints().to_vec()),
        }
    }
}
