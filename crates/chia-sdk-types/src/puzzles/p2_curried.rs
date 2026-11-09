use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Curried arguments for the p2 curried puzzle, which lets a coin be spent by revealing a puzzle
/// with a specific hash.
///
/// When spent, it asserts that the tree hash of the revealed puzzle is `puzzle_hash`, then runs it
/// with the given solution. This is useful when the full puzzle isn't known up front, such as
/// for the clawback puzzle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2CurriedArgs {
    /// The tree hash of the puzzle that must be revealed to spend the coin.
    pub puzzle_hash: Bytes32,
}

impl P2CurriedArgs {
    /// Creates new [`P2CurriedArgs`] from the hash of the puzzle that can spend the coin.
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

/// The solution for the p2 curried puzzle. See [`P2CurriedArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2CurriedSolution<P, S> {
    /// The revealed puzzle, whose tree hash must match the curried puzzle hash.
    pub puzzle: P,
    /// The solution to run the revealed puzzle with.
    pub solution: S,
}

impl<P, S> P2CurriedSolution<P, S> {
    /// Creates a new [`P2CurriedSolution`] from the revealed puzzle and its solution.
    pub fn new(puzzle: P, solution: S) -> Self {
        Self { puzzle, solution }
    }
}

/// The serialized CLVM bytecode of the p2 curried puzzle. See [`P2CurriedArgs`].
pub const P2_CURRIED_PUZZLE: [u8; 143] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff02ffff04ff02ffff04ff0bff80
    80808080ffff01ff02ff0bff1780ffff01ff088080ff0180ffff04ffff01ff02
    ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff
    09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0b
    ffff0101ff058080ff0180ff018080
    "
);

/// The tree hash of [`P2_CURRIED_PUZZLE`].
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
