use std::{array::TryFromSliceError, io, num::TryFromIntError};

use clvm_traits::{FromClvmError, ToClvmError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OfferError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Try from slice error: {0}")]
    TryFromSlice(#[from] TryFromSliceError),

    #[error("Try from int error: {0}")]
    TryFromInt(#[from] TryFromIntError),

    #[error("Missing compression version prefix")]
    MissingVersionPrefix,

    #[error("Unsupported compression version")]
    UnsupportedVersion,

    #[error("Streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[error("Cannot decompress uncompressed input")]
    NotCompressed,

    #[error("Flate2 error: {0}")]
    Flate2(#[from] flate2::DecompressError),

    #[error("Invalid prefix: {0}")]
    InvalidPrefix(String),

    #[error("Encoding is not bech32m")]
    InvalidFormat,

    #[error("Error when decoding address: {0}")]
    Decode(#[from] bech32::Error),

    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("Requested payment puzzle mismatch")]
    PuzzleMismatch,
}
