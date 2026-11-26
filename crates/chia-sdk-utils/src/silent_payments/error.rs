//! Typed errors for the CHIP-0057 silent-payments wallet API.

use thiserror::Error;

/// Errors of the silent payment keys, labels and address codec.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SilentPaymentError {
    /// The HRP is neither `spxch` (mainnet) nor `tspxch` (testnet), for example a standard
    /// `xch1...` address.
    #[error("invalid silent-payment HRP '{0}' (expected 'spxch' or 'tspxch')")]
    WrongHrp(String),

    /// A version 0 payload is not exactly 96 bytes, or a version 1-30 payload is shorter.
    #[error("invalid silent-payment payload length: expected 96 bytes, got {0}")]
    PayloadLength(usize),

    /// The address has version 31, which is reserved; senders must not send to it.
    #[error("silent-payment address version 31 is reserved for a backward-incompatible upgrade")]
    ReservedAddressVersion,

    /// The address is longer than 1,023 characters.
    #[error("silent-payment address too long: {0} characters (max 1023)")]
    AddressTooLong(usize),

    /// A public key is not a valid compressed G1 point in the prime-order subgroup.
    #[error("invalid silent-payment public-key encoding")]
    InvalidPublicKey,

    /// The scan or the spend public key is the identity element.
    #[error("silent-payment public key is the identity element")]
    IdentityPublicKey,

    /// Label 0 is the change label (CHIP-0057 "Change Detection") and has no shareable address.
    #[error("label index 0 is reserved for change outputs and cannot be exposed")]
    ReservedChangeLabel,

    /// A [`super::LabelRegistry`] was used with a scan key other than its own.
    #[error("label registry belongs to another scan key")]
    LabelRegistryKeyMismatch,

    /// A bech32m parse, checksum or character error.
    #[error("bech32m error: {0}")]
    Bech32(#[from] crate::Bech32Error),
}
