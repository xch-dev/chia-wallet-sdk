use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE: [u8; 615] = hex!(
    "
    ff02ffff01ff04ff8202ffffff04ffff04ff14ffff04ffff0117ffff04ffff02
    ff3effff04ff02ffff04ffff04ffff0101ffff04ffff04ff10ffff04ff5fffff
    04ff82017fffff04ffff04ff5fff8080ff8080808080ff808080ff80808080ff
    ff04ffff30ff81bfffff02ff05ffff04ffff02ff16ffff04ff02ffff04ff0bff
    ff04ffff02ff3effff04ff02ffff04ffff04ff5fff8202ff80ff80808080ffff
    04ff17ff808080808080ff8203ff8080ff82017f80ff8080808080ffff04ffff
    04ff18ffff04ffff0112ffff04ffff04ffff0175ffff04ff81bfff808080ffff
    04ff5fff8080808080ff80808080ffff04ffff01ffffff3343ff4202ffffff02
    ffff03ff05ffff01ff0bff7affff02ff2effff04ff02ffff04ff09ffff04ffff
    02ff12ffff04ff02ffff04ff0dff80808080ff808080808080ffff016a80ff01
    80ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c
    7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f5
    96718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d2
    25f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298
    a91ce119a63400ade7c5ffff0bff5affff02ff2effff04ff02ffff04ff05ffff
    04ffff02ff12ffff04ff02ffff04ff07ff80808080ff808080808080ffff0bff
    1cffff0bff1cff6aff0580ffff0bff1cff0bff4a8080ff02ffff03ffff07ff05
    80ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080ffff
    02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080
    ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    fca43ea7bb146208a670bf123bcd7be255a80af67948ba2218fc40e901590168
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatUnlockingPuzzleArgs<CM> {
    pub cat_maker: CM,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
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
