use std::num::TryFromIntError;

use chia_sdk_types::ConditionError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("failed to serialize clvm value: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("failed to deserialize clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("failed to parse conditions: {0}")]
    Conditions(#[from] ConditionError),

    #[error("clvm eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("custom driver error: {0}")]
    Custom(String),

    #[error("invalid mod hash")]
    InvalidModHash,

    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    #[error("missing child")]
    MissingChild,

    #[error("missing hint")]
    MissingHint,

    #[error("missing memo")]
    MissingMemo,

    #[error("invalid memo")]
    InvalidMemo,

    #[error("invalid singleton struct")]
    InvalidSingletonStruct,

    #[error("try from int error")]
    TryFromInt(#[from] TryFromIntError),

    /// An error occurred while reading or writing data.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
