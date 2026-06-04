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

    /// The decoded payload is not 96 bytes. Either too short or too long.
    #[error("invalid silent-payment payload length: expected 96 bytes, got {0}")]
    PayloadLength(usize),

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
