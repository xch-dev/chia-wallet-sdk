use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2CurriedArgs {
    pub puzzle_hash: Bytes32,
}

impl P2CurriedArgs {
    pub fn new(puzzle_hash: Bytes32) -> Self {
        Self { puzzle_hash }
    }
}

impl Mod for P2CurriedArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_CURRIED_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_CURRIED_PUZZLE_HASH
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2CurriedSolution<P, S> {
    pub puzzle: P,
    pub solution: S,
}

impl<P, S> P2CurriedSolution<P, S> {
    pub fn new(puzzle: P, solution: S) -> Self {
        Self { puzzle, solution }
    }
}

pub const P2_CURRIED_PUZZLE: [u8; 143] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff02ffff04ff02ffff04ff0bff80
    80808080ffff01ff02ff0bff1780ffff01ff088080ff0180ffff04ffff01ff02
    ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff
    09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0b
    ffff0101ff058080ff0180ff018080
    "
);

pub const P2_CURRIED_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "13e29a62b42cd2ef72a79e4bacdc59733ca6310d65af83d349360d36ec622363"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_CURRIED_PUZZLE => P2_CURRIED_PUZZLE_HASH);
        Ok(())
    }
}
