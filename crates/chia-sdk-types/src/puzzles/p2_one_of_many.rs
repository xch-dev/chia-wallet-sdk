use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{P2_1_OF_N, P2_1_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{MerkleProof, Mod};

/// Curried arguments for the `p2_1_of_n` puzzle, which lets a coin be spent by any one of a set
/// of puzzles.
///
/// The puzzle hashes are the leaves of a [`MerkleTree`](crate::MerkleTree), and only its root is
/// curried in. To spend the coin, the solution reveals one of the puzzles along with its
/// [`MerkleProof`], and the puzzle is run with the given solution.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_sdk_types::{MerkleTree, Mod, puzzles::{P2OneOfManyArgs, P2OneOfManySolution}};
/// # use clvm_traits::clvm_quote;
/// # use clvm_utils::ToTreeHash;
/// # use clvmr::NodePtr;
/// // A puzzle that always returns nil, and another puzzle hash that can also spend the coin.
/// let puzzle = clvm_quote!(());
/// let puzzle_hash: Bytes32 = puzzle.tree_hash().into();
/// let other_puzzle_hash = Bytes32::new([1; 32]);
///
/// let merkle_tree = MerkleTree::new(&[puzzle_hash, other_puzzle_hash]);
/// let args = P2OneOfManyArgs::new(merkle_tree.root());
/// # let _ = args.curry_tree_hash();
///
/// let proof = merkle_tree.proof(puzzle_hash).expect("the puzzle hash is a leaf");
/// let solution = P2OneOfManySolution::new(proof, puzzle, NodePtr::NIL);
/// # let _ = solution;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2OneOfManyArgs {
    /// The root of the merkle tree of puzzle hashes that can spend the coin.
    pub merkle_root: Bytes32,
}

impl P2OneOfManyArgs {
    /// Creates new [`P2OneOfManyArgs`] from the merkle root.
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Mod for P2OneOfManyArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_1_OF_N)
    }

    fn mod_hash() -> TreeHash {
        P2_1_OF_N_HASH.into()
    }
}

/// The solution for the `p2_1_of_n` puzzle. See [`P2OneOfManyArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2OneOfManySolution<P, S> {
    /// The proof that the tree hash of `puzzle` is a leaf of the merkle tree.
    pub merkle_proof: MerkleProof,
    /// The puzzle to run.
    pub puzzle: P,
    /// The solution to run the puzzle with.
    pub solution: S,
}

impl<P, S> P2OneOfManySolution<P, S> {
    /// Creates a new [`P2OneOfManySolution`] from the merkle proof, puzzle, and solution.
    pub fn new(merkle_proof: MerkleProof, puzzle: P, solution: S) -> Self {
        Self {
            merkle_proof,
            puzzle,
            solution,
        }
    }
}
