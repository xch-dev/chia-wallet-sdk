use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, Mod};

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE: [u8; 629] = hex!(
    "
    ff02ffff01ff02ffff03ff2fffff01ff0880ffff01ff04ff8202ffffff04ffff
    04ff14ffff04ffff0117ffff04ffff02ff3effff04ff02ffff04ffff04ffff01
    01ffff04ffff04ff10ffff04ff5fffff04ff82017fffff04ffff04ff5fff8080
    ff8080808080ff808080ff80808080ffff04ffff30ff81bfffff02ff05ffff04
    ffff02ff16ffff04ff02ffff04ff0bffff04ffff02ff3effff04ff02ffff04ff
    ff04ff5fff8202ff80ff80808080ffff04ff17ff808080808080ff8203ff8080
    ff82017f80ff8080808080ffff04ffff04ff18ffff04ffff0112ffff04ffff0e
    ffff0175ff81bf80ffff04ff5fff8080808080ff8080808080ff0180ffff04ff
    ff01ffffff3343ff4202ffffff02ffff03ff05ffff01ff0bff7affff02ff2eff
    ff04ff02ffff04ff09ffff04ffff02ff12ffff04ff02ffff04ff0dff80808080
    ff808080808080ffff016a80ff0180ffffa04bf5122f344554c53bde2ebb8cd2
    b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124c
    eb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb861929
    1eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471eb
    cb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff0bff5affff02
    ff2effff04ff02ffff04ff05ffff04ffff02ff12ffff04ff02ffff04ff07ff80
    808080ff808080808080ffff0bff1cffff0bff1cff6aff0580ffff0bff1cff0b
    ff4a8080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04
    ff02ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff80808080
    80ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    402c5390423408191049f0a7b3f68c268859ec5dcfc3aa7494cca69d1a8d67d0
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
