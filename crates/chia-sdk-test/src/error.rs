use std::io;

use chia_consensus::validation_error::ErrorCode;
use chia_sdk_signer::SignerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulatorError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Validation error: {0:?}")]
    Validation(ErrorCode),

    #[error("Signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("Missing key")]
    MissingKey,
}
