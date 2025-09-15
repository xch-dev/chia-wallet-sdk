use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE: [u8; 657] = hex!(
    "
    ff02ffff01ff04ff8202ffffff02ff3cffff04ff02ffff04ffff0bffff02ff05
    ffff04ff0bff8203ff8080ffff02ff3effff04ff02ffff04ffff04ffff02ff3e
    ffff04ff02ffff04ffff04ff5fff82017f80ff80808080ffff04ffff02ff2eff
    ff04ff02ffff04ffff02ff3affff04ff02ffff04ff17ffff04ffff02ff3effff
    04ff02ffff04ffff04ff81bfff8202ff80ff80808080ffff04ff2fff80808080
    8080ff80808080ff808080ff8080808080ffff04ffff04ffff04ff10ffff04ff
    82017fff808080ff8080ff808080808080ffff04ffff01ffffff463fff3eff02
    ff04ffff04ff18ffff04ff05ff808080ffff04ffff04ff14ffff04ff05ff8080
    80ff0b8080ffffff02ffff03ff05ffff01ff0bff81eaffff02ff16ffff04ff02
    ffff04ff09ffff04ffff02ff12ffff04ff02ffff04ff0dff80808080ff808080
    808080ffff0181ca80ff0180ffffffa04bf5122f344554c53bde2ebb8cd2b7e3
    d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99
    a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eae
    a194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f
    3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff0bff81aaffff02ff16
    ffff04ff02ffff04ff05ffff04ffff02ff12ffff04ff02ffff04ff07ff808080
    80ff808080808080ffff0bff2cffff0bff2cff81caff0580ffff0bff2cff0bff
    818a8080ffff04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff80808080
    ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff
    04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01
    ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    79d7ed195f419e93a831216216dc68e9f9c8af56d48beef1510c6fb382258bb8
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
