use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// The serialized CLVM bytecode of the p2 parent puzzle. See [`P2ParentArgs`].
pub const P2_PARENT_PUZZLE: [u8; 157] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ffff30ff0bffff02ff05ffff04ffff02ff
    06ffff04ff02ffff04ff2fff80808080ff7f8080ff1780ff808080ffff02ff2f
    ff5f8080ffff04ffff01ff47ff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

/// The tree hash of [`P2_PARENT_PUZZLE`].
pub const P2_PARENT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    638181aa0cd6ea9f042a69e578690513385e8531b361ffc26f03cc35f51018c2
    "
));

/// Curried arguments for the p2 parent puzzle, which lets a coin be spent by the inner puzzle of
/// its parent.
///
/// The solution reveals the parent's inner puzzle and a solution for it. The puzzle runs the CAT
/// maker with `(parent_inner_puzzle_hash . cat_maker_solution)` to get the parent's full puzzle
/// hash, asserts that the parent coin id matches (with `ASSERT_MY_PARENT_ID`), and outputs the
/// conditions of the parent's inner puzzle. So whoever could spend the parent can also spend this
/// coin.
///
/// This puzzle differs from the `p2_parent` puzzle in [`chia_puzzles`], which has a different
/// solution layout.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2ParentArgs<CM> {
    /// A program that converts the parent's inner puzzle hash into its full puzzle hash (such as
    /// by wrapping it in the CAT layer).
    pub cat_maker: CM,
}

/// The solution for the p2 parent puzzle. See [`P2ParentArgs`].
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2ParentSolution<PIP, PS, CMS> {
    /// The parent coin id of the parent coin.
    pub parent_parent_id: Bytes32,
    /// The amount of the parent coin.
    pub parent_amount: u64,
    /// The inner puzzle of the parent coin, which is run to get the conditions.
    pub parent_inner_puzzle: PIP,
    /// The solution to run the parent's inner puzzle with.
    pub parent_solution: PS,
    /// Extra arguments passed to the CAT maker after the parent's inner puzzle hash.
    #[clvm(rest)]
    pub cat_maker_solution: CMS,
}

impl<M> Mod for P2ParentArgs<M> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_PARENT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_PARENT_PUZZLE_HASH
    }
}
