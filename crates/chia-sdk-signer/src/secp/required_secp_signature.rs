use chia_secp::{K1PublicKey, R1PublicKey};
use clvmr::NodePtr;

/// A public key on either of the secp curves supported by CLVM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpPublicKey {
    /// A secp256k1 public key, checked by the `secp256k1_verify` operator.
    K1(K1PublicKey),
    /// A secp256r1 (P-256) public key, checked by the `secp256r1_verify` operator.
    R1(R1PublicKey),
}

/// A secp256k1 or secp256r1 signature requested by a puzzle.
///
/// Unlike BLS signatures, secp signatures aren't aggregated into the spend bundle. The puzzle
/// checks them with the `secp256k1_verify` or `secp256r1_verify` operator, so the signature has
/// to be put in the solution in place of the placeholder that was there when the signatures
/// were calculated.
///
/// The `message_hash` is signed directly (it's prehashed), so it is only meaningful
/// if the puzzle that requested it is trusted. See [`SecpDialect`](super::SecpDialect).
///
/// # Example
///
/// ```
/// use chia_sdk_signer::{RequiredSecpSignature, SecpPublicKey};
/// use chia_secp::{K1SecretKey, K1Signature};
///
/// fn sign(required: &RequiredSecpSignature, secret_key: &K1SecretKey) -> Option<K1Signature> {
///     match required.public_key {
///         SecpPublicKey::K1(public_key) if public_key == secret_key.public_key() => {
///             secret_key.sign_prehashed(&required.message_hash).ok()
///         }
///         _ => None,
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RequiredSecpSignature {
    /// The public key that the puzzle verifies the signature against.
    pub public_key: SecpPublicKey,
    /// The 32 byte message hash that must be signed with `sign_prehashed`.
    pub message_hash: [u8; 32],
    /// The signature argument that was passed to the verify operator, which is usually the
    /// placeholder signature from the solution. It points into the [`Allocator`](clvmr::Allocator)
    /// that the signatures were calculated with.
    pub placeholder_ptr: NodePtr,
}
