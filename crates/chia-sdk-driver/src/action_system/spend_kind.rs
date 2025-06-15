mod conditions_spend;

pub use conditions_spend::*;

use crate::Output;

#[derive(Debug, Clone)]
pub enum SpendKind {
    Conditions(ConditionsSpend),
}

impl SpendKind {
    pub fn conditions() -> Self {
        Self::Conditions(ConditionsSpend::new())
    }

    pub fn can_output(&self, output: &Output) -> bool {
        match self {
            Self::Conditions(spend) => !spend.has_output(output),
        }
    }

    #[must_use]
    pub fn child(&self) -> Self {
        match self {
            Self::Conditions(_spend) => Self::conditions(),
        }
    }
}
