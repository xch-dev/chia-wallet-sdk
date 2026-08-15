use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE: [u8; 497] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ff8205ffffff04ffff04ffff013fffff04ff02ff
    808080ffff04ffff04ffff013effff04ffff0effff016cff0280ff808080ffff
    04ffff04ffff0146ffff04ff8202ffff808080ff8080808080ffff04ffff0bff
    ff02ff05ffff04ff0bff8203ff8080ffff02ff04ffff04ff04ffff04ffff02ff
    04ffff04ff04ffff04ff5fff82017f808080ffff04ffff04ffff0bffff0102ff
    ff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff01820101
    80ff1780ffff0bffff0102ffff02ff06ffff04ff06ffff04ffff02ff04ffff04
    ff04ffff04ff81bfff8202ff808080ffff04ff2fff8080808080ffff0bffff01
    0180808080ffff04ff8202ffffff04ffff04ffff02ff04ffff04ff04ffff04ff
    81bfff2f808080ff8080ff80808080ff808080808080ff018080ffff04ffff04
    ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff
    02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff0380
    80ff0180ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff01820104
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff
    0bffff018201018080ff018080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3e541eddff4520b17742f867c2789528db7a18d99a3bf9e5cc141f12e6ecffd1
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatLockingPuzzleArgs<CM> {
    pub cat_maker: CM,
    pub offer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl<CM> RewardDistributorCatLockingPuzzleArgs<CM> {
    pub fn new(cat_maker: CM, my_p2_puzzle_hash: Bytes32) -> Self {
        Self {
            cat_maker,
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCatLockingPuzzleSolution<CMS> {
    pub my_id: Bytes32,
    pub cat_amount: u64,
    #[clvm(rest)]
    pub cat_maker_solution_rest: CMS,
}

impl<CM> Mod for RewardDistributorCatLockingPuzzleArgs<CM> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH
    }
}
