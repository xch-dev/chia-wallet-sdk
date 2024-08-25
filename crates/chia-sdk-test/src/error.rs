use std::io;

use chia_consensus::gen::validation_error::ErrorCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulatorError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Validation error: {0:?}")]
    Validation(ErrorCode),
}
