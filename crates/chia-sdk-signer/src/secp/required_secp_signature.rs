use clvmr::NodePtr;

use super::SecpPublicKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecpOp {
    K1,
    R1,
}

#[derive(Debug, Clone, Copy)]
pub struct RequiredSecpSignature {
    pub op: SecpOp,
    pub public_key: SecpPublicKey,
    pub message_hash: [u8; 32],
    pub placeholder_ptr: NodePtr,
}
