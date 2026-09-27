use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

pub const P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE: [u8; 479] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff3dffff11ff82017fff5f80ff81bf80ffff01ff08
    80ffff01ff04ffff04ffff0155ffff04ff82017fff808080ffff04ffff04ffff
    0151ffff04ffff11ff82017fff81bf80ff808080ffff04ffff04ffff013fffff
    04ffff0bffff02ff04ffff04ff06ffff04ff05ffff04ff2fffff04ff8207ffff
    808080808080ffff0163ffff0bffff0102ffff0bffff0101ff82017f80ffff0b
    ffff0102ffff0bffff0101ffff02ff04ffff04ff06ffff04ff0bffff04ffff0b
    ffff0101ff8202ff80ffff04ff17ff80808080808080ffff0bffff0101ff8205
    ff80808080ff808080ffff04ffff04ffff0149ffff04ff8205ffff808080ffff
    04ffff04ffff0146ffff04ff8202ffff808080ff80808080808080ff0180ffff
    04ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff
    0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff
    04ff02ff078080ffff0bffff010180808080ffff01ff02ffff03ff03ffff01ff
    0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0b
    ffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ff
    ff0bffff010180808080ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    23522d15e71b71040b05a0ab6be2580ae2b62f72d70c7e83c1cd8aa515437a06
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2NextRewardDistributorEpochArgs {
    pub singleton_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub clawback_inner_puzzle_hash: Bytes32,
    pub reward_distributor_singleton_struct_hash: Bytes32,
    pub reward_distributor_first_epoch_start: u64,
    pub reward_distributor_epoch_seconds: u64,
}

impl P2NextRewardDistributorEpochArgs {
    pub fn new(
        clawback_inner_puzzle_hash: Bytes32,
        reward_distributor_singleton_struct_hash: TreeHash,
        reward_distributor_first_epoch_start: u64,
        reward_distributor_epoch_seconds: u64,
    ) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            clawback_inner_puzzle_hash,
            reward_distributor_singleton_struct_hash: reward_distributor_singleton_struct_hash
                .into(),
            reward_distributor_first_epoch_start,
            reward_distributor_epoch_seconds,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2NextRewardDistributorEpochSolution {
    pub next_epoch_start: u64,
    pub my_id: Bytes32,
    pub my_amount: u64,
    #[clvm(rest)]
    pub reward_distributor_inner_puzzle_hash: Bytes32,
}

impl Mod for P2NextRewardDistributorEpochArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE_HASH
    }
}
