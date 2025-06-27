use std::{array::TryFromSliceError, num::TryFromIntError};

use chia_sdk_signer::SignerError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("try from int error")]
    TryFromInt(#[from] TryFromIntError),

    #[error("try from slice error: {0}")]
    TryFromSlice(#[from] TryFromSliceError),

    #[error("failed to serialize clvm value: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("failed to deserialize clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("clvm eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("invalid mod hash")]
    InvalidModHash,

    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    #[error("missing child")]
    MissingChild,

    #[error("missing hint")]
    MissingHint,

    #[error("missing memo")]
    MissingMemo,

    #[error("invalid memo")]
    InvalidMemo,

    #[error("invalid singleton struct")]
    InvalidSingletonStruct,

    #[error("expected even oracle fee, but it was odd")]
    OddOracleFee,

    #[error("custom driver error: {0}")]
    Custom(String),

    #[error("invalid merkle proof")]
    InvalidMerkleProof,

    #[error("unknown puzzle")]
    UnknownPuzzle,

    #[error("invalid spend count for vault subpath")]
    InvalidSubpathSpendCount,

    #[error("missing spend for vault subpath")]
    MissingSubpathSpend,

    #[error("delegated puzzle wrapper conflict")]
    DelegatedPuzzleWrapperConflict,

    #[error("cannot emit conditions from spend")]
    CannotEmitConditions,

    #[error("cannot settle from spend")]
    CannotSettleFromSpend,

    #[error("singleton spend already finalized")]
    AlreadyFinalized,

    #[error("there is no spendable source coin that can create the output without a conflict")]
    NoSourceForOutput,

    #[error("invalid asset id")]
    InvalidAssetId,

    #[error("missing key")]
    MissingKey,

    #[cfg(feature = "offer-compression")]
    #[error("missing compression version prefix")]
    MissingVersionPrefix,

    #[cfg(feature = "offer-compression")]
    #[error("unsupported compression version")]
    UnsupportedVersion,

    #[cfg(feature = "offer-compression")]
    #[error("streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    #[cfg(feature = "offer-compression")]
    #[error("cannot decompress uncompressed input")]
    NotCompressed,

    #[cfg(feature = "offer-compression")]
    #[error("flate2 error: {0}")]
    Flate2(#[from] flate2::DecompressError),

    #[cfg(feature = "offer-compression")]
    #[error("invalid prefix: {0}")]
    InvalidPrefix(String),

    #[cfg(feature = "offer-compression")]
    #[error("encoding is not bech32m")]
    InvalidFormat,

    #[cfg(feature = "offer-compression")]
    #[error("error when decoding address: {0}")]
    Decode(#[from] bech32::Error),

    #[error("incompatible asset info")]
    IncompatibleAssetInfo,

    #[error("missing required singleton asset info")]
    MissingAssetInfo,

    #[error("conflicting inputs in offers")]
    ConflictingOfferInputs,

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),
}
