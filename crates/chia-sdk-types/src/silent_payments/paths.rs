//! EIP-2333 derivation paths for the CHIP-0057 silent-payment scan and spend
//! keys (CHIP-0057 "Key Derivation").
//!
//! ```text
//! m/12381n/8444n/12n/0n   scan secret key  (b_scan)
//! m/12381n/8444n/13n/0n   spend secret key (b_spend)
//! ```
//!
//! Every level is **hardened** (the `n` suffix is the notation Chia tooling
//! uses for a hardened, non-observer step). CHIP-0057 forbids unhardened
//! derivation for these keys: an unhardened child secret key, combined with
//! the parent public key, reveals the parent secret key, so a leaked scan key
//! and the wallet's master public key would together reveal every key in the
//! wallet.
//!
//! Purpose indices `12` (scan) and `13` (spend) are distinct from the indices
//! Chia already uses (`2` for the standard wallet). The final index `0` is
//! reserved for future use as an account number.

/// Derivation path of the silent-payment scan secret key,
/// `m/12381n/8444n/12n/0n`. Every index is derived hardened.
pub const SCAN_PATH: &[u32] = &[12381, 8444, 12, 0];

/// Derivation path of the silent-payment spend secret key,
/// `m/12381n/8444n/13n/0n`. Every index is derived hardened.
pub const SPEND_PATH: &[u32] = &[12381, 8444, 13, 0];
