use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, Mod};

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE: [u8; 671] = hex!(
    "
    ff02ffff01ff04ff8202ffffff02ff3cffff04ff02ffff04ffff0bffff02ff05
    ffff04ff0bff8203ff8080ffff02ff3effff04ff02ffff04ffff04ffff02ff3e
    ffff04ff02ffff04ffff04ff5fff82017f80ff80808080ffff04ffff02ff2eff
    ff04ff02ffff04ffff02ff3affff04ff02ffff04ff17ffff04ffff02ff3effff
    04ff02ffff04ffff04ff81bfff8202ff80ff80808080ffff04ff2fff80808080
    8080ffff04ff8202ffff8080808080ff808080ff8080808080ffff04ffff04ff
    ff04ff10ffff04ff82017fff808080ff8080ff808080808080ffff04ffff01ff
    ffff463fff3eff02ff04ffff04ff18ffff04ff05ff808080ffff04ffff04ff14
    ffff04ffff0effff016cff0580ff808080ff0b8080ffffff02ffff03ff05ffff
    01ff0bff81eaffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff12ffff04
    ff02ffff04ff0dff80808080ff808080808080ffff0181ca80ff0180ffffffa0
    4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a
    a09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7
    b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f68069
    23f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119
    a63400ade7c5ff0bff81aaffff02ff16ffff04ff02ffff04ff05ffff04ffff02
    ff12ffff04ff02ffff04ff07ff80808080ff808080808080ffff0bff2cffff0b
    ff2cff81caff0580ffff0bff2cff0bff818a8080ffff04ff05ffff04ff0bffff
    04ffff04ff05ff8080ff80808080ff02ffff03ffff07ff0580ffff01ff0bffff
    0102ffff02ff3effff04ff02ffff04ff09ff80808080ffff02ff3effff04ff02
    ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    614190050c503354a1223aa5168bd2ea1ceb7ae2e8f8a69f2f59974544d71e3d
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
