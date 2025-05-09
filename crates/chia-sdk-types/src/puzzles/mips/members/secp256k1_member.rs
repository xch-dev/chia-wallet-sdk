use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Secp256k1Member {
    pub public_key: K1PublicKey,
}

impl Secp256k1Member {
    pub fn new(public_key: K1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for Secp256k1Member {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256K1_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        SECP256K1_MEMBER_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct Secp256k1MemberSolution {
    pub coin_id: Bytes32,
    pub signature: K1Signature,
}

impl Secp256k1MemberSolution {
    pub fn new(coin_id: Bytes32, signature: K1Signature) -> Self {
        Self { coin_id, signature }
    }
}

pub const SECP256K1_MEMBER: [u8; 53] = hex!(
    "
    ff02ffff01ff04ffff04ff02ffff04ff17ff808080ffff8413d61f00ff05ffff
    0bff0bff1780ff2f8080ffff04ffff0146ff018080
    "
);

pub const SECP256K1_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "2b05daf134c9163acc8f2ac05b61f7d8328fca3dcc963154a28e89bcfc4dbfca"
));
