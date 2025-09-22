use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BulletinArgs {
    pub id: Bytes32,
}

impl BulletinArgs {
    pub fn new(id: Bytes32) -> Self {
        Self { id }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BulletinSolution<M, S> {
    pub message: M,
    pub signature: S,
}

impl<M, S> BulletinSolution<M, S> {
    pub fn new(message: M, signature: S) -> Self {
        Self { message, signature }
    }
}

impl Mod for BulletinArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&BULLETIN_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        BULLETIN_PUZZLE_HASH
    }
}

pub const BULLETIN_PUZZLE: [u8; 155] = hex!(
    "
    ff02ffff01ff04ffff04ffff013cffff04ffff0bffff02ff02ffff04ff02ffff
    04ff0bff80808080ffff02ff02ffff04ff02ffff04ff17ff8080808080ff8080
    80ff8080ffff04ffff01ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff
    02ff02ffff04ff02ffff04ff09ff80808080ffff02ff02ffff04ff02ffff04ff
    0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const BULLETIN_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "7e89bc03f9e25818d26a44cb205bc8e843833fe28ee362b4c0b7063bc0d7cf2c"
));
