use clvm_traits::{FromClvmError, ToClvmError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OfferError {
    #[error("from clvm error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("to clvm error: {0}")]
    ToClvm(#[from] ToClvmError),
}
