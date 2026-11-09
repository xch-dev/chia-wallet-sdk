use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized uniqueness prelauncher puzzle, compiled from Rue.
///
/// A prelauncher is a 0-amount coin, created by a registry singleton, that creates a
/// singleton launcher with amount 1 and announces a value. Since the value is curried
/// into the prelauncher, a launcher id can be checked to come from a given registry
/// coin and value, which proves that the singleton (for example a `CATalog` NFT)
/// represents that value.
///
/// Curried with [`UniquenessPrelauncher1stCurryArgs`], then
/// [`UniquenessPrelauncher2ndCurryArgs`]. It takes no solution.
pub const UNIQUENESS_PRELAUNCHER_PUZZLE: [u8; 45] = hex!(
    "
    ff04ffff04ffff0133ffff04ff02ffff01ff01808080ffff04ffff04
    ffff013effff04ff05ff808080ff808080
    "
);

/// Tree hash of [`UNIQUENESS_PRELAUNCHER_PUZZLE`].
pub const UNIQUENESS_PRELAUNCHER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    e7fa797a8537895c571ae9556abdd2ebaba2d36957f2c63f37aba130f8f0c5ff
    "
));

/// First curry of the uniqueness prelauncher ([`UNIQUENESS_PRELAUNCHER_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher1stCurryArgs {
    /// Puzzle hash of the singleton launcher that the prelauncher creates.
    pub launcher_puzzle_hash: Bytes32,
}

/// Second curry of the uniqueness prelauncher.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher2ndCurryArgs<V> {
    /// Value the prelauncher represents and announces, such as an asset id.
    pub value: V,
}

impl Mod for UniquenessPrelauncher1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&UNIQUENESS_PRELAUNCHER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        UNIQUENESS_PRELAUNCHER_PUZZLE_HASH
    }
}
