use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE: [u8; 555] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff20ff2f80ffff15ff8202ffff808080ffff
    01ff04ff8202ffffff04ffff04ffff0142ffff04ffff0117ffff04ffff02ff04
    ffff04ff04ffff04ffff0101ffff04ffff04ffff0133ffff04ff5fffff04ff82
    017fffff04ffff04ff5fff8080ff8080808080ffff01ffff52ff808080808080
    80ffff04ffff30ff81bfffff02ff05ffff04ff17ff8203ff8080ff82017f80ff
    8080808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0175
    ff81bf80ffff04ff5fff8080808080ffff04ffff04ffff0142ffff04ffff0112
    ffff04ff80ffff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102
    ffff0bffff0102ffff0bffff0182010180ff0b80ffff0bffff0102ffff02ff06
    ffff04ff06ffff04ffff0bffff0101ffff02ff04ffff04ff04ffff04ff5fffff
    04ff8202ffff82017f8080808080ff80808080ffff0bffff010180808080ff80
    80808080ff8080808080ffff01ff088080ff0180ffff04ffff04ffff01ff02ff
    ff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ff
    ff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff
    01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201
    018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    7146fbf50b1a688eb6263f76d93faee874030d26c8562e3538dab9e806498e5b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatUnlockingPuzzleArgs<CM> {
    pub cat_maker: CM,
    pub deposit_slot_1st_curry_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl<CM> RewardDistributorCatUnlockingPuzzleArgs<CM> {
    pub fn new(
        cat_maker: CM,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
    ) -> Self {
        Self {
            cat_maker,
            deposit_slot_1st_curry_hash,
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
