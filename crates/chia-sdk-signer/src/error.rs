use chia_sdk_types::ConditionError;
use clvm_traits::ToClvmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("condition error: {0}")]
    Condition(#[from] ConditionError),

    #[error("clvm error")]
    ToClvm(#[from] ToClvmError),

    #[error("infinity public key")]
    InfinityPublicKey,
}
