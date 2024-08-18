use std::{array::TryFromSliceError, io, num::TryFromIntError};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("{0}")]
    TryFromSlice(#[from] TryFromSliceError),

    #[error("missing version prefix")]
    MissingVersionPrefix,

    #[error("unsupported version")]
    UnsupportedVersion,

    #[error("streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[error("cannot decompress uncompressed input")]
    NotCompressed,

    #[error("flate2 error: {0}")]
    Flate2(#[from] flate2::DecompressError),

    #[error("cast error: {0}")]
    Cast(#[from] TryFromIntError),
}
