use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{M_OF_N, M_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS `m_of_n` puzzle, which requires `required` of the members in a
/// merkle tree to approve a delegated puzzle hash.
///
/// The leaves of the merkle tree are the members' puzzle hashes. The solution is a partially
/// revealed tree in which each approving member is revealed with its solution. Each of them is
/// run with the delegated puzzle hash prepended to its solution, and their conditions are
/// combined. The puzzle asserts that the tree has the curried root and that exactly `required`
/// members were run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct MofNArgs {
    /// The number of members that must approve a spend.
    pub required: usize,
    /// The root of the merkle tree of member puzzle hashes.
    pub merkle_root: Bytes32,
}

impl MofNArgs {
    /// Creates new [`MofNArgs`] from the number of required members and the merkle root.
    pub fn new(required: usize, merkle_root: Bytes32) -> Self {
        Self {
            required,
            merkle_root,
        }
    }
}

impl Mod for MofNArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&M_OF_N)
    }

    fn mod_hash() -> TreeHash {
        M_OF_N_HASH.into()
    }
}

/// The solution for the MIPS `m_of_n` puzzle. See [`MofNArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNSolution<P> {
    /// The partially revealed merkle tree. A revealed member is
    /// `(() member_puzzle . member_solution)`, a branch is `(left . right)`, and a subtree that
    /// isn't revealed is replaced by its hash.
    pub proofs: P,
}

impl<P> MofNSolution<P> {
    /// Creates a new [`MofNSolution`] from the partially revealed merkle tree.
    pub fn new(proofs: P) -> Self {
        Self { proofs }
    }
}
