use clvm_traits::FromClvmError;
use thiserror::Error;

use crate::ConditionError;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to parse clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("condition error: {0}")]
    Condition(#[from] ConditionError),

    #[error("invalid mod hash")]
    InvalidModHash,

    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    #[error("missing child")]
    MissingChild,

    #[error("missing hint")]
    MissingHint,

    #[error("invalid singleton struct")]
    InvalidSingletonStruct,

    #[error("mismatched singleton output (maybe no spend revealed the new singleton state)")]
    MismatchedOutput,
}
