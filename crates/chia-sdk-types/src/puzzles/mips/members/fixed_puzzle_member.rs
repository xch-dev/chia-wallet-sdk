use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct FixedPuzzleMember {
    pub fixed_puzzle_hash: Bytes32,
}

impl FixedPuzzleMember {
    pub fn new(fixed_puzzle_hash: Bytes32) -> Self {
        Self { fixed_puzzle_hash }
    }
}

impl Mod for FixedPuzzleMember {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FIXED_PUZZLE_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        FIXED_PUZZLE_MEMBER_HASH
    }
}

pub const FIXED_PUZZLE_MEMBER: [u8; 25] =
    hex!("ff02ffff03ffff09ff02ff0580ff80ffff01ff088080ff0180");

pub const FIXED_PUZZLE_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "34ede3eadc52ed750e405f2b9dea9891506547f651290bb606356d997c64f219"
));
