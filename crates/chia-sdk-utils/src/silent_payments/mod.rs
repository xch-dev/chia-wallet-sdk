//! Silent payments (CHIP-0057) — wallet-facing key derivation and bech32m address encoding.
//!
//! This module contains the public types a wallet author uses to:
//!   - derive `(scan_sk, spend_sk)` from a BIP-39 mnemonic with hardened
//!     derivation at the CHIP-0057 paths `m/12381n/8444n/12n/0n` and
//!     `m/12381n/8444n/13n/0n` ([`SilentPaymentKeys::from_mnemonic`]),
//!   - build the key bundle from explicit secret keys
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
/// `label_scalar = int(tagged_hash("Chia_SP/Label", ser256(b_scan) || ser32(m)))
/// mod r` and `label_pk = label_scalar * G` (CHIP-0057 "Label Generation"). The
/// scanner adds the scalar to the output tweak of a labeled detection.
///
/// `m = 0` (the change label) is accepted here; only
/// [`SilentPaymentKeys::labeled_address`] rejects it, to keep the change address
/// from being handed out.
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
