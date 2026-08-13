//! BIP-32-style unhardened derivation paths for CHIP-0057 silent-payment
//! scan and spend keys.
//!
//! ```text
//! m/12381/8444/12/0   scan secret key  (b_scan)
//! m/12381/8444/13/0   spend secret key (b_spend)
//! ```
//!
//! Indices `12` (scan) and `13` (spend) are CHIP-0057 reserved values,
//! distinct from index `2` used by the standard Chia wallet.

/// Unhardened derivation path for the silent-payment scan secret key:
/// `m/12381/8444/12/0`.
pub const SCAN_PATH: &[u32] = &[12381, 8444, 12, 0];

/// Unhardened derivation path for the silent-payment spend secret key:
/// `m/12381/8444/13/0`.
pub const SPEND_PATH: &[u32] = &[12381, 8444, 13, 0];
