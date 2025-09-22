use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_MEMBER, BLS_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsMember {
    pub public_key: PublicKey,
}

impl BlsMember {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for BlsMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_MEMBER_HASH.into()
    }
}
