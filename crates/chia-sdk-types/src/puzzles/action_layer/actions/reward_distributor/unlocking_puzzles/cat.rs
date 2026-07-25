use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE: [u8; 493] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ff2fffff01ff0880ffff01ff04ff8202ffffff04ffff
    04ffff0142ffff04ffff0117ffff04ffff02ff04ffff04ff04ffff04ffff0101
    ffff04ffff04ffff0133ffff04ff5fffff04ff82017fffff04ffff04ff5fff80
    80ff8080808080ffff01ffff52ff80808080808080ffff04ffff30ff81bfffff
    02ff05ffff04ffff0bffff0102ffff0bffff0182
    010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0b80ffff
    0bffff0102ffff02ff06ffff04ff06ffff04ffff02ff04ffff04ff04ffff04ff
    5fff8202ff808080ffff04ff17ff8080808080ffff0bffff010180808080ff82
    03ff8080ff82017f80ff8080808080ffff04ffff04ffff0143ffff04ffff0112
    ffff04ffff0effff0175ff81bf80ffff04ff5fff8080808080ff8080808080ff
    0180ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ff
    ff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff
    0bffff0101ff038080ff0180ffff01ff02ffff03ff03ffff01ff0bffff0102ff
    ff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff01820101
    80ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff0101
    80808080ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0ce068fb8fe1a7e745be4d0b512db82fd1f9c144b4521ab06a5c12930e15b6ae
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatUnlockingPuzzleArgs<CM> {
    pub cat_maker: CM,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl<CM> RewardDistributorCatUnlockingPuzzleArgs<CM> {
    pub fn new(cat_maker: CM, my_p2_puzzle_hash: Bytes32) -> Self {
        Self {
            cat_maker,
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
        }
    }
}
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCatUnlockingPuzzleSolution<CMS> {
    pub cat_parent_id: Bytes32,
    pub cat_amount: u64,
    pub cat_shares: u64,
    #[clvm(rest)]
    pub cat_maker_solution_rest: CMS,
}

impl<CM> Mod for RewardDistributorCatUnlockingPuzzleArgs<CM> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH
    }
}
