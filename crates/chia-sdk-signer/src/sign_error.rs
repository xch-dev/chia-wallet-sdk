use clvm_traits::FromClvmError;
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignError {
    #[error("{0:?}")]
    Eval(#[from] EvalErr),

    #[error("{0}")]
    Clvm(#[from] FromClvmError),

    #[error("infinity public key")]
    InfinityPublicKey,
}
