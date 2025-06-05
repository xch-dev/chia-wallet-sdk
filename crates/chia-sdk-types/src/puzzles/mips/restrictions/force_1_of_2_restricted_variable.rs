use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{
    puzzles::{DELEGATED_FEEDER_PUZZLE_HASH, INDEX_WRAPPER_HASH, ONE_OF_N_PUZZLE_HASH},
    Mod,
};

use super::RESTRICTIONS_PUZZLE_HASH;

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
            delegated_puzzle_feeder_mod_hash: DELEGATED_FEEDER_PUZZLE_HASH.into(),
            one_of_n_mod_hash: ONE_OF_N_PUZZLE_HASH.into(),
            left_side_subtree_hash_hash: left_side_subtree_hash.tree_hash().into(),
            index_wrapper_mod_hash: INDEX_WRAPPER_HASH.into(),
            nonce,
            restriction_mod_hash: RESTRICTIONS_PUZZLE_HASH.into(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        }
    }
}

impl Mod for Force1of2RestrictedVariable {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FORCE_1_OF_2_RESTRICTED_VARIABLE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        FORCE_1_OF_2_RESTRICTED_VARIABLE_PUZZLE_HASH
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

pub const FORCE_1_OF_2_RESTRICTED_VARIABLE_PUZZLE: [u8; 650] = hex!(
    "
    ff02ffff01ff02ffff03ffff02ff12ffff04ff02ffff04ff8205ffffff04ffff
    02ff16ffff04ff02ffff04ff2fffff04ffff0bff18ff5f80ffff04ffff02ff16
    ffff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff0bffff04ff
    ff0bff18ffff0bff14ff17ffff0bff18ffff02ff16ffff04ff02ffff04ff2fff
    ff04ffff0bff18ff5f80ffff04ffff02ff16ffff04ff02ffff04ff81bfffff04
    ff82017fffff04ff8202ffffff04ff820bffff80808080808080ff8080808080
    80808080ff8080808080ff8080808080ff808080808080ff8080808080ffff01
    8205ffffff01ff088080ff0180ffff04ffff01ffffff3301ff02ff02ffff03ff
    05ffff01ff0bff7affff02ff1effff04ff02ffff04ff09ffff04ffff02ff1cff
    ff04ff02ffff04ff0dff80808080ff808080808080ffff016a80ff0180ffffff
    02ffff03ff05ffff01ff02ffff03ffff02ffff03ffff09ff11ff1080ffff01ff
    02ffff03ffff20ffff09ff29ff0b8080ffff01ff0101ff8080ff0180ff8080ff
    0180ffff01ff0880ffff01ff02ff12ffff04ff02ffff04ff0dffff04ff0bff80
    8080808080ff0180ffff01ff010180ff0180ffffa04bf5122f344554c53bde2e
    bb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a
    73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb
    8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fb
    a471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff0bff5a
    ffff02ff1effff04ff02ffff04ff05ffff04ffff02ff1cffff04ff02ffff04ff
    07ff80808080ff808080808080ff0bff14ffff0bff14ff6aff0580ffff0bff14
    ff0bff4a8080ff018080
    "
);

pub const FORCE_1_OF_2_RESTRICTED_VARIABLE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "4f7bc8f30deb6dad75a1e29ceacb67fd0fe0eda79173e45295ff2cfbb8de53c6"
));
