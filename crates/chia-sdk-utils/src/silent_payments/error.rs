//! Typed errors for the CHIP-0057 silent-payments wallet API.

use thiserror::Error;

/// Errors produced by the silent-payments wallet API (key derivation, label
/// generation, address encode/decode).
///
/// The `Bech32` variant wraps the existing `chia_sdk_utils::Bech32Error` so
/// callers can pattern-match on the underlying bech32m parse / checksum /
/// character errors without an extra layer of nesting.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SilentPaymentError {
    /// Decoded HRP is neither `"spxch"` (mainnet) nor `"tspxch"` (testnet).
    /// Specifically catches `"xch1..."` (the standard Chia address) being
    /// passed to [`super::SilentPaymentAddress::decode`].
    #[error("invalid silent-payment HRP '{0}' (expected 'spxch' or 'tspxch')")]
    WrongHrp(String),

    /// The decoded payload has an invalid length: a v0 payload must be exactly
    /// 96 bytes; a v1-30 (forward-compatible) payload must be at least 96 bytes.
    #[error("invalid silent-payment payload length: expected 96 bytes, got {0}")]
    PayloadLength(usize),

    /// The address carries version 31, reserved by CHIP-0057 for a
    /// backward-incompatible upgrade. Senders MUST NOT send to it.
    #[error("silent-payment address version 31 is reserved for a backward-incompatible upgrade")]
    ReservedAddressVersion,

    /// The address exceeds the CHIP-0057 length allowance of 1,023 characters.
    #[error("silent-payment address too long: {0} characters (max 1023)")]
    AddressTooLong(usize),

    /// One of the 48-byte pubkey halves failed `chia_bls::PublicKey::from_bytes`
    /// (e.g., not a valid compressed G1 point).
    #[error("invalid silent-payment public-key encoding")]
    InvalidPublicKey,

    /// Either the scan or spend pubkey decoded to the BLS identity element
    /// (point at infinity). CHIP §215 mandates rejection to prevent trivial-
    /// secret-key griefing.
    #[error("silent-payment public key is the identity element")]
    IdentityPublicKey,

    /// `SilentPaymentKeys::labeled_address(0)` was called. `m = 0` is reserved
    /// as the change label (CHIP §125-§130) and must not appear in a publicly-
    /// shared address.
    #[error("label index 0 is reserved for change outputs and cannot be exposed")]
    ReservedChangeLabel,

    /// bech32 / bech32m parse, checksum, or character error. Wraps the
    /// existing `chia_sdk_utils::Bech32Error` so the error message stays
    /// uniform with the standard-address path.
    #[error("bech32m error: {0}")]
    Bech32(#[from] crate::Bech32Error),
}
