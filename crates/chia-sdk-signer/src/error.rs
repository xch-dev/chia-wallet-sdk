use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("Infinity public key")]
    InfinityPublicKey,

    #[error("Invalid secp key")]
    InvalidSecpKey(#[from] k256::ecdsa::Error),
}
