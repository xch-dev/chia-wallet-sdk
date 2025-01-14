use chia_secp::{K1PublicKey, R1PublicKey};
use clvmr::NodePtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpPublicKey {
    K1(K1PublicKey),
    R1(R1PublicKey),
}

#[derive(Debug, Clone, Copy)]
pub struct RequiredSecpSignature {
    pub public_key: SecpPublicKey,
    pub message_hash: [u8; 32],
    pub placeholder_ptr: NodePtr,
}
