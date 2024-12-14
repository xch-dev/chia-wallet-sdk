use chia_sdk_types::{Secp256k1PublicKey, Secp256r1PublicKey};
use clvmr::NodePtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpPublicKey {
    K1(Secp256k1PublicKey),
    R1(Secp256r1PublicKey),
}

#[derive(Debug, Clone, Copy)]
pub struct RequiredSecpSignature {
    pub public_key: SecpPublicKey,
    pub message_hash: [u8; 32],
    pub placeholder_ptr: NodePtr,
}
