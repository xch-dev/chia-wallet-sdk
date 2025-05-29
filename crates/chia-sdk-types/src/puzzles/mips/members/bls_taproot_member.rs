use std::borrow::Cow;

use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

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
        Cow::Borrowed(&BLS_TAPROOT_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        BLS_TAPROOT_MEMBER_HASH
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

pub const BLS_TAPROOT_MEMBER: [u8; 99] = hex!(
    "
    ff02ffff01ff02ffff03ff17ffff01ff02ffff03ffff09ff05ffff1dff17ffff
    1effff0bff17ff0b80808080ff80ffff01ff088080ff0180ffff01ff04ffff04
    ff02ffff04ff05ffff04ff0bff80808080ff808080ff0180ffff04ffff0132ff
    018080
    "
);

pub const BLS_TAPROOT_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "35d2ad31aaf0df91c965909e5112294c57a18354ee4a5aae80572080ec3b6842"
));
