use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash_atom, TreeHash};
use hex_literal::hex;

use crate::{Mod, DELEGATED_FEEDER_PUZZLE_HASH, INDEX_WRAPPER_HASH, VAULT_1_OF_N_PUZZLE_HASH};

use super::RESTRICTIONS_PUZZLE_HASH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Recovery {
    pub delegated_puzzle_feeder_mod_hash: Bytes32,
    pub one_of_n_mod_hash: Bytes32,
    pub left_side_subtree_hash_hash: Bytes32,
    pub index_wrapper_mod_hash: Bytes32,
    pub nonce: usize,
    pub restriction_mod_hash: Bytes32,
    pub member_validator_list_hash: Bytes32,
    pub delegated_puzzle_validator_list_hash: Bytes32,
}

impl Recovery {
    pub fn new(
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
    ) -> Self {
        Self {
            delegated_puzzle_feeder_mod_hash: DELEGATED_FEEDER_PUZZLE_HASH.into(),
            one_of_n_mod_hash: VAULT_1_OF_N_PUZZLE_HASH.into(),
            left_side_subtree_hash_hash: tree_hash_atom(&left_side_subtree_hash).into(),
            index_wrapper_mod_hash: INDEX_WRAPPER_HASH.into(),
            nonce,
            restriction_mod_hash: RESTRICTIONS_PUZZLE_HASH.into(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        }
    }
}

impl Mod for Recovery {
    const MOD_REVEAL: &[u8] = &RECOVERY_PUZZLE;
    const MOD_HASH: TreeHash = RECOVERY_PUZZLE_HASH;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct RecoverySolution {
    pub new_right_side_member_hash: Bytes32,
}

impl RecoverySolution {
    pub fn new(new_right_side_member_hash: Bytes32) -> Self {
        Self {
            new_right_side_member_hash,
        }
    }
}

pub const RECOVERY_PUZZLE: [u8; 684] = hex!(
    "
    ff02ffff01ff02ffff03ffff02ff3affff04ff02ffff04ff8205ffffff04ffff
    02ff2effff04ff02ffff04ff2fffff04ffff0bff2cff5f80ffff04ffff02ff2e
    ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff0bffff04ff
    ff0bff2cffff0bff12ff17ffff0bff2cffff02ff2effff04ff02ffff04ff2fff
    ff04ffff0bff2cff5f80ffff04ffff02ff2effff04ff02ffff04ff81bfffff04
    ff82017fffff04ff8202ffffff04ff820bffff80808080808080ff8080808080
    80808080ff8080808080ff8080808080ff808080808080ff8080808080ffff01
    8205ffffff01ff088080ff0180ffff04ffff01ffffff333cff3eff0142ffff02
    ffff02ffff03ff05ffff01ff0bff76ffff02ff3effff04ff02ffff04ff09ffff
    04ffff02ff2affff04ff02ffff04ff0dff80808080ff808080808080ffff0166
    80ff0180ff02ffff03ff05ffff01ff02ffff03ffff21ffff09ff11ff3c80ffff
    09ff11ff1480ffff09ff11ff1880ffff02ffff03ffff09ff11ff1080ffff01ff
    02ffff03ffff20ffff09ff29ff0b8080ffff01ff0101ff8080ff0180ff8080ff
    018080ffff01ff0880ffff01ff02ff3affff04ff02ffff04ff0dffff04ff0bff
    808080808080ff0180ffff01ff010180ff0180ffffffa04bf5122f344554c53b
    de2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623
    d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee2
    10fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd
    63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff0b
    ff56ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2affff04ff02ffff
    04ff07ff80808080ff808080808080ff0bff12ffff0bff12ff66ff0580ffff0b
    ff12ff0bff468080ff018080
    "
);

pub const RECOVERY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "8b71805f8e559ab8df3342691d5be28a6e57c541778753598fc88d7591699bd1"
));
