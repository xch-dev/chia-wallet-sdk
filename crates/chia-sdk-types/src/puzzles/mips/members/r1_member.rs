use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct R1Member {
    pub public_key: R1PublicKey,
}

impl R1Member {
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for R1Member {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&R1_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        R1_MEMBER_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct R1MemberSolution {
    pub coin_id: Bytes32,
    pub signature: R1Signature,
}

impl R1MemberSolution {
    pub fn new(coin_id: Bytes32, signature: R1Signature) -> Self {
        Self { coin_id, signature }
    }
}

pub const R1_MEMBER: [u8; 53] = hex!(
    "
    ff02ffff01ff04ffff04ff02ffff04ff17ff808080ffff841c3a8f00ff05ffff
    0bff0bff1780ff2f8080ffff04ffff0146ff018080
    "
);

pub const R1_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "05aaa1f2fb6c48b5bce952b09f3da99afa4241989878a9919aafb7d74b70ac54"
));
