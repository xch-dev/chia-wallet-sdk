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

    #[error("missing vault coin spend in transaction reveal")]
    MissingVaultCoinSpend,

    #[cfg(feature = "chip-0057")]
    #[error("silent payment error: {0}")]
    SilentPayment(#[from] chia_sdk_utils::silent_payments::SilentPaymentError),

    /// A silent-payment send needs the synthetic secret key for every spent XCH
    /// input. Some input's key was missing, so multi-party aggregation would be
    /// required — multi-party silent-payment flows are not currently supported.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment multi-party flow unsupported")]
    SilentPaymentMultiPartyUnsupported,

    /// The silent-payment send had no wallet-controlled (non-ephemeral) XCH
    /// input to bind the output to. At least one is required.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment requires an xch input")]
    SilentPaymentNoXchInputs,

    /// The first memo was exactly 32 bytes, which the standard wallet promotes
    /// to a `puzzle_hash` hint and indexes — exposing the one-time puzzle hash
    /// and defeating silent-payment privacy. Prefix the payload with a sentinel
    /// byte so the first atom is no longer 32 bytes.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment memo hint forbidden")]
    SilentPaymentMemoHintForbidden,

    /// A multi-input silent-payment send (2+ non-ephemeral XCH inputs) must pass
    /// `Relation::AssertConcurrent` to `Spends::finish_with_keys` so the
    /// receiver can reconstruct the input set; single-input sends accept any
    /// `Relation`.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment requires input binding")]
    SilentPaymentRequiresInputBinding,

    /// `Spends::with_silent_payment_keys` was not called before finish, so no
    /// silent-payment secret keys are registered for the spent inputs.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment keys not registered")]
    SilentPaymentKeysNotRegistered,

    /// A registered silent-payment key is not the synthetic key for its coin.
    /// `StandardArgs::curry_tree_hash(registered_pk)` must equal the coin's
    /// `p2_puzzle_hash` and `registered_sk.public_key()` must equal
    /// `registered_pk`. Pass synthetic keys (`derive_synthetic`) or construct
    /// them via `SyntheticSecretKey::from_raw`.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment key not synthetic")]
    SilentPaymentKeyNotSynthetic,

    /// A silent-payment send was co-bundled with one or more non-XCH asset
    /// spends (CAT / DID / NFT / option) in the same bundle. Silent-payment
    /// send bundles must be XCH-only; co-spending other assets in the same
    /// bundle is not supported.
    #[cfg(feature = "chip-0057")]
    #[error(
        "silent-payment sends must be XCH-only bundles; co-spending CAT/DID/NFT/option coins is not supported"
    )]
    SilentPaymentMixedAssetBundle,
}
