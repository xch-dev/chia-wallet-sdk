use chia_secp::{K1PublicKey, R1PublicKey};
use clvmr::NodePtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpPublicKey {
    K1(K1PublicKey),
    R1(R1PublicKey),
}

/// A secp256k1 or secp256r1 signature requested by a puzzle.
///
/// The `message_hash` is signed directly (it's prehashed), so it is only meaningful
/// if the puzzle that requested it is trusted. See [`SecpDialect`](super::SecpDialect).
#[derive(Debug, Clone, Copy)]
pub struct RequiredSecpSignature {
    pub public_key: SecpPublicKey,
    pub message_hash: [u8; 32],
    pub placeholder_ptr: NodePtr,
}
