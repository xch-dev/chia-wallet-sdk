//! Silent payments (CHIP-0057) — wallet-facing key derivation and bech32m address encoding.
//!
//! This module contains the public types a wallet author uses to:
//!   - derive `(scan_sk, spend_sk)` from a BIP-39 mnemonic at the CHIP-0057
//!     paths `m/12381/8444/12/0` and `m/12381/8444/13/0` ([`SilentPaymentKeys::from_mnemonic`]),
//!   - build a watch-only / key-import setup from raw secret keys
//!     ([`SilentPaymentKeys::from_secret_keys`]),
//!   - encode and decode silent-payment addresses as bech32m with HRP
//!     `spxch` (mainnet) / `tspxch` (testnet) over the 96-byte
//!     `serialize(B_scan) || serialize(B_spend)` payload
//!     ([`SilentPaymentAddress::encode`], [`SilentPaymentAddress::decode`]),
//!   - generate labeled sub-addresses ([`SilentPaymentKeys::labeled_address`])
//!     and maintain a `label_pk → label_index` registry ([`LabelRegistry`])
//!     for the scanner to attribute labeled detections.
//!
//! All scalar-field reduction in this module flows through
//! [`chia_sdk_types::silent_payments::ScalarField`], which enforces the
//! unsigned-vs-signed byte-interpretation choice at the type level. See
//! `ScalarField::from_bytes_unsigned` for why unsigned reduction is mandatory
//! for protocol scalars.

mod address;
pub use address::*;
mod error;
pub use error::*;
mod keys;
pub use keys::*;
mod labels;
pub use labels::*;

/// Compute the CHIP-0057 label scalar and label public key for label index `m`.
///
/// Public reach-through over [`labels::generate_label`] (which is `pub(crate)`
/// so cross-module consumers in this crate can reach it but external callers
/// must go through this wrapper).
///
/// Used by `chia-sdk-driver`'s silent-payment scanner to compute the labeled
/// `onetime_sk = base_onetime_sk + label_scalar` for labeled detections.
///
/// `m = 0` is accepted here — the public-API change-label rejection lives in
/// [`SilentPaymentKeys::labeled_address`]. The change label (`m = 0`) is used
/// internally to register own-change detection.
#[must_use]
pub fn generate_label(
    scan_sk: &chia_bls::SecretKey,
    m: u32,
) -> (
    chia_sdk_types::silent_payments::ScalarField,
    chia_bls::PublicKey,
) {
    labels::generate_label(scan_sk, m)
}
