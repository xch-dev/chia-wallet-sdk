use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// The splitter puzzle from CHIP-0008, as compiled in the reference implementation
/// (`p2_royalty_share_arbitrary_shares_rest_to_last.clsp`).
pub const SPLITTER_PUZZLE: [u8; 317] = hex!(
    "
    ff02ffff01ff04ffff04ff0affff01ff808080ffff04ffff04ff08ffff04ff0b
    ff808080ffff02ff16ffff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04
    ff80ffff04ff0bff80808080808080808080ffff04ffff01ffff4933ff3cffff
    02ffff03ff05ffff01ff02ff1effff04ff02ffff04ff05ffff04ffff05ffff14
    ffff12ff0bff2980ff178080ffff04ff0bffff04ff17ffff04ff2fffff04ff5f
    ff808080808080808080ffff01ff02ffff03ffff09ff17ff2f80ff80ffff01ff
    088080ff018080ff0180ff04ffff04ff0cffff04ff11ffff04ffff02ffff03ff
    0dffff010bffff0181bf80ff0180ffff04ffff04ff11ff8080ff8080808080ff
    ff02ff16ffff04ff02ffff04ff0dffff04ff17ffff04ff2fffff04ffff10ff5f
    ff2980ffff04ffff11ff81bfff0b80ff808080808080808080ff018080
    "
);

/// The tree hash of [`SPLITTER_PUZZLE`].
pub const SPLITTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    1b08dc6c15f642cb0445c7c68e8df10da090d262c793839b2d62c04e7be6d0a0
    "
));

/// A recipient in a splitter's payout scheme.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SplitterShare {
    /// The puzzle hash that this recipient's coin is created with, which is also used as its hint.
    pub puzzle_hash: Bytes32,
    /// The number of shares this recipient receives, out of the sum of all shares.
    pub share: u64,
}

impl SplitterShare {
    /// Creates a new [`SplitterShare`] that pays `share` shares to `puzzle_hash`.
    pub fn new(puzzle_hash: Bytes32, share: u64) -> Self {
        Self { puzzle_hash, share }
    }
}

/// Curried arguments of [`SPLITTER_PUZZLE`], which splits a coin's amount between recipients
/// according to a fixed payout scheme, for example to share NFT royalties between recipients.
///
/// Anyone can spend a splitter coin, since it can only pay out according to the scheme. Each
/// recipient receives `my_amount * share / total_shares` (rounded down), except for the last
/// one, which receives the rest so that the full amount is paid out. Each coin is hinted with its
/// puzzle hash.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_sdk_types::{Mod, puzzles::{SplitterArgs, SplitterShare}};
/// // An 80/20 split between two puzzle hashes.
/// let args = SplitterArgs::new(vec![
///     SplitterShare::new(Bytes32::new([1; 32]), 80),
///     SplitterShare::new(Bytes32::new([2; 32]), 20),
/// ]);
///
/// // Coins sent to this puzzle hash can be split by anyone.
/// let splitter_puzzle_hash = args.curry_tree_hash();
/// # let _ = splitter_puzzle_hash;
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct SplitterArgs {
    /// The recipients and their shares, in the order that the coins are created.
    pub payout_scheme: Vec<SplitterShare>,
}

impl SplitterArgs {
    /// Creates new [`SplitterArgs`] with the given payout scheme.
    pub fn new(payout_scheme: Vec<SplitterShare>) -> Self {
        Self { payout_scheme }
    }
}

/// The solution of the [`SplitterArgs`] puzzle.
///
/// The spend also creates a coin announcement with an empty message.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SplitterSolution {
    /// The amount of the coin being split, which the puzzle asserts.
    pub my_amount: u64,
    /// The sum of the shares in the payout scheme. The spend fails if this doesn't match.
    pub total_shares: u64,
}

impl SplitterSolution {
    /// Creates a new [`SplitterSolution`] for a coin with the given amount.
    pub fn new(my_amount: u64, total_shares: u64) -> Self {
        Self {
            my_amount,
            total_shares,
        }
    }
}

impl Mod for SplitterArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SPLITTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        SPLITTER_PUZZLE_HASH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_splitter_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(SPLITTER_PUZZLE => SPLITTER_PUZZLE_HASH);
        Ok(())
    }
}
