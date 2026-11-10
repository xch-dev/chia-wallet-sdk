use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    DELEGATED_PUZZLE_FEEDER_HASH, FORCE_1_OF_2_W_RESTRICTED_VARIABLE,
    FORCE_1_OF_2_W_RESTRICTED_VARIABLE_HASH, ONE_OF_N_HASH, RESTRICTIONS_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{Mod, puzzles::INDEX_WRAPPER_HASH};

/// Curried arguments for the MIPS `force_1_of_2_w_restricted_variable` wrapper, which forces every
/// created coin to use a 1-of-2 vault with a fixed left side and a restricted right side.
///
/// The right side is a new member chosen in the solution, wrapped in fixed restrictions. Every
/// `CREATE_COIN` must have the puzzle hash of an
/// [`IndexWrapperArgs`](crate::puzzles::IndexWrapperArgs) (with the nonce) around a
/// [`DelegatedPuzzleFeederArgs`](crate::puzzles::DelegatedPuzzleFeederArgs) around a
/// [`OneOfNArgs`](crate::puzzles::OneOfNArgs) whose merkle tree has the left side and the right
/// side. The right side is an index wrapper (with the same nonce) around a
/// [`RestrictionsArgs`](crate::puzzles::RestrictionsArgs) with the curried validators and the new
/// member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Force1of2RestrictedVariable {
    /// The mod hash of the delegated puzzle feeder.
    pub delegated_puzzle_feeder_mod_hash: Bytes32,
    /// The mod hash of the `1_of_n` puzzle.
    pub one_of_n_mod_hash: Bytes32,
    /// The tree hash of the left side's hash, which is the left branch of the merkle tree.
    pub left_side_subtree_hash_hash: Bytes32,
    /// The mod hash of the index wrapper.
    pub index_wrapper_mod_hash: Bytes32,
    /// The nonce used by both index wrappers.
    pub nonce: usize,
    /// The mod hash of the restrictions puzzle.
    pub restriction_mod_hash: Bytes32,
    /// The tree hash of the list of member validators for the right side.
    pub member_validator_list_hash: Bytes32,
    /// The tree hash of the list of delegated puzzle validators for the right side.
    pub delegated_puzzle_validator_list_hash: Bytes32,
}

impl Force1of2RestrictedVariable {
    /// Creates new [`Force1of2RestrictedVariable`] from the hash of the left side, the nonce, and
    /// the tree hashes of the right side's validator lists, using the standard MIPS mod hashes.
    pub fn new(
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
    ) -> Self {
        Self {
            delegated_puzzle_feeder_mod_hash: DELEGATED_PUZZLE_FEEDER_HASH.into(),
            one_of_n_mod_hash: ONE_OF_N_HASH.into(),
            left_side_subtree_hash_hash: left_side_subtree_hash.tree_hash().into(),
            index_wrapper_mod_hash: INDEX_WRAPPER_HASH.into(),
            nonce,
            restriction_mod_hash: RESTRICTIONS_HASH.into(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        }
    }
}

impl Mod for Force1of2RestrictedVariable {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_1_OF_2_W_RESTRICTED_VARIABLE)
    }

    fn mod_hash() -> TreeHash {
        FORCE_1_OF_2_W_RESTRICTED_VARIABLE_HASH.into()
    }
}

/// The solution for the MIPS `force_1_of_2_w_restricted_variable` wrapper. See
/// [`Force1of2RestrictedVariable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Force1of2RestrictedVariableSolution {
    /// The puzzle hash of the new member on the right side, before it's restricted.
    pub new_right_side_member_hash: Bytes32,
}

impl Force1of2RestrictedVariableSolution {
    /// Creates a new [`Force1of2RestrictedVariableSolution`] from the new right side member hash.
    pub fn new(new_right_side_member_hash: Bytes32) -> Self {
        Self {
            new_right_side_member_hash,
        }
    }
}
