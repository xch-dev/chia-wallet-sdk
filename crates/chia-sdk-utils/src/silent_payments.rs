//! Silent payments (CHIP-0057): key derivation ([`SilentPaymentKeys`]), the bech32m address
//! codec ([`SilentPaymentAddress`]) and labels ([`LabelRegistry`]).

mod address;
pub use address::*;
mod error;
pub use error::*;
mod keys;
pub use keys::*;
mod labels;
pub use labels::*;

/// The label scalar and label public key for label index `m` (CHIP-0057 "Label Generation").
/// `m = 0`, the change label, is accepted; only [`SilentPaymentKeys::labeled_address`] rejects it.
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
