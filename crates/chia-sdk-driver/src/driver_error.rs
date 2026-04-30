use std::{array::TryFromSliceError, num::TryFromIntError};

use chia_sdk_signer::SignerError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::error::EvalErr;
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

    #[error("missing spend")]
    MissingSpend,

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
    #[error("decompressed output exceeds maximum allowed size")]
    DecompressionTooLarge,

    #[cfg(feature = "offer-compression")]
    #[error("flate2 error: {0}")]
    Flate2(#[from] flate2::DecompressError),

    #[cfg(feature = "offer-compression")]
    #[error("error when decoding address: {0}")]
    Decode(#[from] chia_sdk_utils::Bech32Error),

    #[error("incompatible asset info")]
    IncompatibleAssetInfo,

    #[error("missing required singleton asset info")]
    MissingAssetInfo,

    #[error("conflicting inputs in offers")]
    ConflictingOfferInputs,

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("invalid delegated spend format")]
    InvalidDelegatedSpendFormat,

    #[error("invalid vault message format")]
    InvalidVaultMessageFormat,

    #[error("puzzle hash mismatch for coin spend")]
    WrongPuzzleHash,

    #[error("nested clawbacks are not allowed")]
    NestedClawback,

    #[error("invalid custody for linked spend")]
    InvalidLinkedCustody,

    #[error("the transaction is not guaranteed to expire when its clawed back spends expire")]
    UnguaranteedClawBack,

    #[error("child is wrapped in unexpected revocation layer")]
    RevocableChild,

    #[error("conflicting vault launcher ids")]
    ConflictingVaultLauncherIds,

    #[error("conditions do not match message")]
    WrongConditions,

    #[error("receive message conditions are not allowed in p2 conditions or singleton puzzles")]
    ReceiveMessageConditionsNotAllowed,

    #[error("vault message did not match any custody auth or TAIL invocation")]
    UnmatchedVaultMessage,

    #[error("multiple vault messages matched the same custody slot")]
    DuplicateVaultMessage,

    #[error("wrong linked offer launcher id")]
    WrongLinkedOfferLauncherId,

    #[error("linked offer coin creates non-settlement payment")]
    InvalidLinkedOfferPayment,

    #[error("offer pre-split coin has the wrong output amount")]
    WrongOfferPreSplitOutput,

    #[error("conflicting puzzle assertions in linked offer")]
    ConflictingLinkedOfferPuzzleAssertions,

    #[error("missing required bulletin conditions")]
    MissingBulletinConditions,
}
