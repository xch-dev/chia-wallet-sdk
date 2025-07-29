use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE: [u8; 1031] = hex!(
    "
    ff02ffff01ff04ffff04ff8209ffffff04ffff11ff8215ffffff11ff829dffff
    8302fbff8080ffff04ffff11ff822dffffff010180ff823dff808080ffff04ff
    ff04ff2cffff04ffff0117ffff04ffff02ff2effff04ff02ffff04ffff04ffff
    0101ffff04ffff04ff18ffff04ff8303fbffffff04ffff0101ffff04ffff04ff
    8303fbffff8080ff8080808080ff808080ff80808080ffff04ffff30ff822bff
    ffff02ff3affff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff
    ff04ff05ffff04ff8213ffff0b8080ff80808080ffff04ffff02ff3affff04ff
    02ffff04ff17ffff04ffff0bffff0101ff1780ffff04ff825bffffff04ff82bb
    ffffff04ffff02ff3affff04ff02ffff04ff2fffff04ffff0bffff0101ff2f80
    ffff04ff818affff04ff83017bffffff04ffff02ff3affff04ff02ffff04ff5f
    ffff04ffff0bffff0101ff8303fbff80ffff04ff81bfff808080808080ff8080
    808080808080ff8080808080808080ff808080808080ffff010180ff80808080
    80ffff04ffff04ff14ffff04ffff0112ffff04ff8213ffffff04ff8303fbffff
    8080808080ffff04ffff04ff10ffff04ffff10ff83013dffff8202ff80ff8080
    80ffff04ffff02ff3effff04ff02ffff04ff82017fffff04ffff02ff2effff04
    ff02ffff04ffff04ff8303fbffffff04ff8302fbffffff01018080ff80808080
    ff8080808080ffff04ffff04ffff0181d6ffff04ff18ffff04ff8303fbffffff
    04ffff11ff829dffff8302fbff80ffff04ffff04ff8303fbffff8080ff808080
    808080ff80808080808080ffff04ffff01ffffff5533ff43ff4202ffffff02ff
    ff03ff05ffff01ff0bff81eaffff02ff16ffff04ff02ffff04ff09ffff04ffff
    02ff12ffff04ff02ffff04ff0dff80808080ff808080808080ffff0181ca80ff
    0180ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cc
    e23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d
    78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b237
    59d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e
    7298a91ce119a63400ade7c5ff0bff81aaffff02ff16ffff04ff02ffff04ff05
    ffff04ffff02ff12ffff04ff02ffff04ff07ff80808080ff808080808080ffff
    0bff3cffff0bff3cff81caff0580ffff0bff3cff0bff818a8080ffff02ffff03
    ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80
    808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff01
    01ff058080ff0180ff04ff2cffff04ffff0112ffff04ff80ffff04ffff02ff3a
    ffff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080
    808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5ea6690901ecc9932c463041090e37afe56c9be3e8a6ff0cbb37a7ad157802e2
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorUnstakeActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_hash: Bytes32,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorUnstakeActionSolution {
    pub nft_launcher_id: Bytes32,
    pub nft_parent_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_transfer_porgram_hash: Bytes32,
    pub entry_initial_cumulative_payout: u64,
    #[clvm(rest)]
    pub entry_custody_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorUnstakeActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH
    }
}
