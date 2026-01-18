use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{RewardDistributorEntrySlotValue, NONCE_WRAPPER_PUZZLE_HASH},
    MerkleProof, Mod,
};

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE: [u8; 2001] = hex!(
    "
    ff02ffff01ff04ffff04ff8209ffffff04ffff11ff8215ffff83017bff80ffff
    04ffff10ff822dffff8302fbff80ffff04ffff04ff829dffffff10ff82ddffff
    8305fbff8080ffff04ff82bdffff808080808080ffff04ffff04ff20ffff04ff
    ff10ff83013dffff82017f80ff808080ffff04ffff04ff30ffff04ffff0bffff
    02ff2affff04ff02ffff04ff09ffff04ffff02ff2effff04ff02ffff04ff05ff
    80808080ffff04ffff02ff2affff04ff02ffff04ff0bffff04ffff0bffff0101
    ff0b80ffff04ffff0bffff0102ffff0bffff0101ff8213ff80ffff02ffff03ff
    822bffffff01822bffffff01819c80ff018080ffff04ff825bffffff04ff82bb
    ffff8080808080808080ff808080808080ffff012480ff808080ffff02ff26ff
    ff04ff02ffff04ff03ffff04ff8307fbffffff04ff83017bffffff04ff8302fb
    ffffff04ff8305fbffff8080808080808080808080ffff04ffff01ffffffff55
    3fff333effff4202ffff02ffff03ff05ffff01ff0bff81fcffff02ff3affff04
    ff02ffff04ff09ffff04ffff02ff2cffff04ff02ffff04ff0dff80808080ff80
    8080808080ffff0181dc80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7
    e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb
    99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291e
    aea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb
    1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffffff04ff28ffff
    04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff8080808080ff04ff28ff
    ff04ffff02ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff80
    80808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bff81bc
    ffff02ff3affff04ff02ffff04ff05ffff04ffff02ff2cffff04ff02ffff04ff
    07ff80808080ff808080808080ff0bff34ffff0bff34ff81dcff0580ffff0bff
    34ff0bff819c8080ffffff02ffff03ff0bffff01ff02ffff03ffff22ffff09ff
    ff12ff81e3ffff11ff83013bfdff81a38080ffff10ffff12ff53ff8205fd80ff
    81b38080ffff15ff81b3ffff0181ff80ffff15ff8205fdff81b380ffff20ffff
    15ff820173ff81e3808080ffff01ff04ffff02ff7effff04ff02ffff04ff8201
    7dffff04ffff02ff2effff04ff02ffff04ff23ff80808080ff8080808080ffff
    04ffff02ff32ffff04ff02ffff04ff82017dffff04ffff02ff2effff04ff02ff
    ff04ffff04ff43ffff04ff83013bfdffff10ff81e3ff820173808080ff808080
    80ffff04ff43ff808080808080ffff04ffff04ffff0181d6ffff04ff28ffff04
    ff43ffff04ff53ffff04ffff04ff43ff8080ff808080808080ffff02ff36ffff
    04ff02ffff04ff05ffff04ff23ffff04ff8201f3ffff04ff820173ffff04ffff
    02ff26ffff04ff02ffff04ff05ffff04ff1bffff04ffff11ff17ff5380ffff04
    ffff11ff2fff82017380ffff04ffff11ff5fff81b380ff8080808080808080ff
    8080808080808080808080ffff01ff088080ff0180ffff01ff21ff17ff5f8080
    ff0180ff02ffff03ff17ffff01ff04ffff04ff24ffff04ffff0117ffff04ffff
    02ff2effff04ff02ffff04ffff04ffff0101ffff04ffff02ff22ffff04ff02ff
    ff04ffff02ff2affff04ff02ffff04ff5dffff04ffff02ff2effff04ff02ffff
    04ffff04ff13ff81a780ff80808080ffff04ff81bdff808080808080ff808080
    80ff808080ff80808080ffff04ffff30ff820167ffff02ff2affff04ff02ffff
    04ff11ffff04ffff02ff2effff04ff02ffff04ffff04ff11ffff04ff8202e7ff
    398080ff80808080ffff04ffff02ff2affff04ff02ffff04ff15ffff04ffff0b
    ffff0101ff1580ffff04ff8205e7ffff04ff820be7ffff04ffff02ff2affff04
    ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff82
    2fe780ffff04ff8217e7ffff04ffff02ff2affff04ff02ffff04ff5dffff04ff
    ff02ff2effff04ff02ffff04ffff04ff13ffff11ff81a7ff478080ff80808080
    ffff04ff81bdff808080808080ff8080808080808080ff8080808080808080ff
    808080808080ffff010180ff8080808080ffff04ffff04ff38ffff04ffff0eff
    ff0172ff8202e780ff808080ffff02ffff03ffff22ffff09ff8227fdffff02ff
    5effff04ff02ffff04ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04
    ff8202e7ff81a780ff8080808080ffff04ff823fe7ff808080808080ff4780ff
    ff01ff02ff36ffff04ff02ffff04ff05ffff04ff0bffff04ff37ffff04ffff11
    ff2fff4780ffff04ff5fff8080808080808080ffff01ff088080ff01808080ff
    ff01ff02ffff03ff2fffff01ff0880ffff015f80ff018080ff0180ffff02ffff
    03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff
    80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff
    0101ff058080ff0180ffff02ffff03ff1bffff01ff02ff5effff04ff02ffff04
    ffff02ffff03ffff18ffff0101ff1380ffff01ff0bffff0102ff2bff0580ffff
    01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13ffff0181ff
    80ff3b80ff8080808080ffff010580ff0180ff04ff24ffff04ffff0112ffff04
    ff80ffff04ffff02ff2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b
    80ff8080808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    65d4372efa6c354f9f7c6dc22f64b2a4d7aaa9457f1008cb0c57252153257d83
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRefreshNftsFromDlActionArgs {
    pub dl_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
    pub precision: u64,
}

impl RewardDistributorRefreshNftsFromDlActionArgs {
    pub fn new(
        dl_launcher_id: Bytes32,
        my_p2_puzzle_hash: Bytes32,
        entry_slot_1st_curry_hash: Bytes32,
        max_second_offset: u64,
        precision: u64,
    ) -> Self {
        Self {
            dl_singleton_struct: SingletonStruct::new(dl_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
            entry_slot_1st_curry_hash,
            max_second_offset,
            precision,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RefreshNftInfo {
    pub nft_shares_delta: i64,
    pub new_nft_shares: u64,
    pub nft_parent_id: Bytes32,
    pub nft_launcher_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_transfer_porgram_hash: Bytes32,
    pub nft_owner: Option<Bytes32>,
    #[clvm(rest)]
    pub nft_inclusion_proof: MerkleProof,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotAndNfts {
    pub existing_slot_value: RewardDistributorEntrySlotValue,
    pub entry_payout_amount: u64,
    pub payout_rounding_error: u64,
    pub nfts_total_shares_delta: i64,
    #[clvm(rest)]
    pub nfts: Vec<RefreshNftInfo>,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRefreshNftsFromDlActionSolution {
    pub dl_root_hash: Bytes32,
    pub dl_metadata_rest_hash: Option<Bytes32>,
    pub dl_metadata_updater_hash_hash: Bytes32,
    pub dl_inner_puzzle_hash: Bytes32,
    pub total_entry_payout_amount: u64,
    pub total_shares_delta: i64,
    pub total_payout_rounding_error: u64,
    #[clvm(rest)]
    pub slots_and_nfts: Vec<SlotAndNfts>,
}

impl Mod for RewardDistributorRefreshNftsFromDlActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH
    }
}
