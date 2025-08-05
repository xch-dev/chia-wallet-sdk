use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{BLS_WITH_TAPROOT_MEMBER, BLS_WITH_TAPROOT_MEMBER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BlsTaprootMember {
    pub synthetic_key: PublicKey,
}

impl BlsTaprootMember {
    pub fn new(synthetic_key: PublicKey) -> Self {
        Self { synthetic_key }
    }
}

impl Mod for BlsTaprootMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BLS_WITH_TAPROOT_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_WITH_TAPROOT_MEMBER_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BlsTaprootMemberSolution {
    pub original_public_key: Option<PublicKey>,
}

impl BlsTaprootMemberSolution {
    pub fn new(original_public_key: Option<PublicKey>) -> Self {
        Self {
            original_public_key,
        }
    }
}
