//! Derivation paths of the scan and spend keys (CHIP-0057 "Key Derivation"). Every level must
//! be derived hardened: an unhardened scan key and the master public key reveal the wallet's keys.

/// The path of the scan secret key, `m/12381n/8444n/12n/0n`, hardened at every level.
pub const SCAN_PATH: &[u32] = &[12381, 8444, 12, 0];

/// The path of the spend secret key, `m/12381n/8444n/13n/0n`, hardened at every level.
pub const SPEND_PATH: &[u32] = &[12381, 8444, 13, 0];
