use chia_sdk_types::conditions::ConditionError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use thiserror::Error;

// todo
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to serialize clvm value: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("failed to deserialize clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("failed to parse conditions: {0}")]
    Conditions(#[from] ConditionError),

    #[error("clvm eval error: {0}")]
    Eval(#[from] EvalErr),

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
