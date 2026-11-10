use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{ONE_OF_N, ONE_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{MerkleProof, Mod};

/// Curried arguments for the MIPS `1_of_n` puzzle, which lets any one member in a merkle tree
/// approve a delegated puzzle hash.
///
/// The leaves of the [`MerkleTree`](crate::MerkleTree) are the members' puzzle hashes. The solution
/// reveals one member along with its [`MerkleProof`], and the member is run with the delegated
/// puzzle hash prepended to its solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct OneOfNArgs {
    /// The root of the merkle tree of member puzzle hashes.
    pub merkle_root: Bytes32,
}

impl OneOfNArgs {
    /// Creates new [`OneOfNArgs`] from the merkle root.
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Mod for OneOfNArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ONE_OF_N)
    }

    fn mod_hash() -> TreeHash {
        ONE_OF_N_HASH.into()
    }
}

/// The solution for the MIPS `1_of_n` puzzle. See [`OneOfNArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct OneOfNSolution<P, S> {
    /// The proof that the tree hash of `member_puzzle` is a leaf of the merkle tree.
    pub merkle_proof: MerkleProof,
    /// The member puzzle to run.
    pub member_puzzle: P,
    /// The solution for the member puzzle, which comes after the delegated puzzle hash.
    pub member_solution: S,
}

impl<P, S> OneOfNSolution<P, S> {
    /// Creates a new [`OneOfNSolution`] from the merkle proof, member puzzle, and member solution.
    pub fn new(merkle_proof: MerkleProof, member_puzzle: P, member_solution: S) -> Self {
        Self {
            merkle_proof,
            member_puzzle,
            member_solution,
        }
    }
}
