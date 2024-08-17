use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConditionError {
    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),
    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),
    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),
}
