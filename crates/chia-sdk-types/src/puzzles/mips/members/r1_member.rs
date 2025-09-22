use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256R1_MEMBER, SECP256R1_MEMBER_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

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
        Cow::Borrowed(&SECP256R1_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        SECP256R1_MEMBER_HASH.into()
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
