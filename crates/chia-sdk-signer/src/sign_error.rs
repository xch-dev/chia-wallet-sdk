use chia_sdk_parser::ConditionError;
use clvm_traits::ToClvmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignError {
    #[error("condition error: {0}")]
    Condition(#[from] ConditionError),

    #[error("to clvm error: {0}")]
    Clvm(#[from] ToClvmError),

    #[error("infinity public key")]
    InfinityPublicKey,
}
