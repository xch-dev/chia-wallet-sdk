use chia_sdk_parser::ParseError;
use clvm_traits::ToClvmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("clvm error")]
    ToClvm(#[from] ToClvmError),

    #[error("infinity public key")]
    InfinityPublicKey,
}
