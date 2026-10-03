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

    #[error("metadata updater puzzle hash mismatch")]
    MetadataUpdaterPuzzleHashMismatch,

    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    #[error("invalid state schedule: must be nonempty and strictly increasing by timestamp")]
    InvalidStateSchedule,

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

    #[error("fee overflow")]
    FeeOverflow,

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

    #[error("the selected coins are insufficient to cover the outputs of the transaction")]
    InsufficientFunds,

    #[error("the amount does not match the amount of the singleton")]
    SingletonAmountMismatch,

    #[error("the CAT does not have a revocation layer")]
    NotRevocable,

    #[error("the amount is too large to fit in a coin")]
    AmountOverflow,

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

    #[error("linked spend has unknown custody puzzle but was sent a message")]
    InvalidLinkedCustody,

    #[error("the transaction is not guaranteed to expire when its clawed back spends expire")]
    UnguaranteedClawBack,

    #[error("the revocation layer of the child does not match the parent")]
    RevocableChild,

    #[error("conflicting vault launcher ids")]
    ConflictingVaultLauncherIds,

    #[error("conditions do not match message")]
    WrongConditions,

    #[error("vault message did not match any custody auth or TAIL invocation")]
    UnmatchedVaultMessage,

    #[error("missing message for linked custody puzzle")]
    MissingVaultMessage,

    #[error("multiple vault messages matched the same custody slot")]
    DuplicateVaultMessage,

    #[error("wrong linked offer launcher id")]
    WrongLinkedOfferLauncherId,

    #[error("missing required bulletin conditions")]
    MissingBulletinConditions,

    #[error(
        "p2 conditions or singleton spend is missing AssertMyCoinId condition to prevent swapping the spend path"
    )]
    MissingP2ConditionsOrSingletonAssertion,

    #[cfg(feature = "chip-0057")]
    #[error("silent payment error: {0}")]
    SilentPayment(#[from] chia_sdk_utils::silent_payments::SilentPaymentError),

    /// A silent-payment send needs the synthetic secret key for every spent XCH
    /// input. Some input's key was missing, so multi-party aggregation would be
    /// required — multi-party silent-payment flows are not currently supported.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment multi-party flow unsupported")]
    SilentPaymentMultiPartyUnsupported,

    /// The silent-payment send had no XCH coin spent with the standard puzzle
    /// to derive the output from. At least one is required.
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

    /// A silent-payment send whose transaction spends two or more XCH coins
    /// (counting intermediate coins that are created and spent inside it) must
    /// pass `Relation::AssertConcurrent` to `Spends::prepare` /
    /// `Spends::finish_with_keys`, so that scanners can reconstruct the spend
    /// group. A transaction that spends a single coin accepts any `Relation`.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment requires input binding")]
    SilentPaymentRequiresInputBinding,

    /// An intermediate coin (one that is created and spent inside the
    /// transaction) is part of the silent-payment spend group, but no secret
    /// key was registered for its puzzle hash. Its key is part of the key sum
    /// a scanner computes, so the payment would be undetectable without it.
    #[cfg(feature = "chip-0057")]
    #[error(
        "silent payment spend group contains an intermediate coin with no registered secret key"
    )]
    SilentPaymentIntermediateKeyMissing,

    /// A silent-payment output would be created by a coin that is not part of
    /// the spend group, such as a settlement coin. CHIP-0057 requires every
    /// silent-payment output to be created by a standard-puzzle coin of the
    /// group.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment output must be created by a standard-puzzle coin of the spend group")]
    SilentPaymentParentNotEligible,

    /// The coins that were bound together when the transaction was completed
    /// differ from the spend group the silent-payment outputs were derived
    /// from, so the payment would be undetectable.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment spend group changed after the outputs were derived")]
    SilentPaymentInputSetChanged,

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
