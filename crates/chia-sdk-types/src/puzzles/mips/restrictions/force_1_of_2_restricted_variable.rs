use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    DELEGATED_PUZZLE_FEEDER_HASH, FORCE_1_OF_2_W_RESTRICTED_VARIABLE,
    FORCE_1_OF_2_W_RESTRICTED_VARIABLE_HASH, ONE_OF_N_HASH, RESTRICTIONS_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{puzzles::INDEX_WRAPPER_HASH, Mod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Force1of2RestrictedVariable {
    pub delegated_puzzle_feeder_mod_hash: Bytes32,
    pub one_of_n_mod_hash: Bytes32,
    pub left_side_subtree_hash_hash: Bytes32,
    pub index_wrapper_mod_hash: Bytes32,
    pub nonce: usize,
    pub restriction_mod_hash: Bytes32,
    pub member_validator_list_hash: Bytes32,
    pub delegated_puzzle_validator_list_hash: Bytes32,
}

impl Force1of2RestrictedVariable {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Force1of2RestrictedVariableSolution {
    pub new_right_side_member_hash: Bytes32,
}

impl Force1of2RestrictedVariableSolution {
    pub fn new(new_right_side_member_hash: Bytes32) -> Self {
        Self {
            new_right_side_member_hash,
        }
    }
}
