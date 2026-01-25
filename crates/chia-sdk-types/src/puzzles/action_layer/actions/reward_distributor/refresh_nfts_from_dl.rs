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

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE: [u8; 1992] = hex!(
    "
    ff02ffff01ff04ffff04ff8209ffffff04ffff11ff8215ffff824fff80ffff04
    ffff10ff822dffff82afff80ffff04ffff04ff829dffffff10ff82ddffff82ef
    ff8080ffff04ff82bdffff808080808080ffff04ffff04ff20ffff04ffff10ff
    83013dffff82017f80ff808080ffff04ffff04ff30ffff04ffff0bffff02ff2a
    ffff04ff02ffff04ff09ffff04ffff02ff2effff04ff02ffff04ff05ff808080
    80ffff04ffff02ff2affff04ff02ffff04ff0bffff04ffff0bffff0101ff0b80
    ffff04ffff0bffff0102ffff0bffff0101ff820bff80ffff02ffff03ff8227ff
    ffff018227ffffff01819c80ff018080ffff04ff8257ffffff04ff8277ffff80
    80808080808080ff808080808080ffff012480ff808080ffff02ff26ffff04ff
    02ffff04ff03ffff04ff823fffffff04ff824fffffff04ff82afffffff04ff82
    efffff8080808080808080808080ffff04ffff01ffffffff553fff333effff42
    02ffff02ffff03ff05ffff01ff0bff81fcffff02ff3affff04ff02ffff04ff09
    ffff04ffff02ff2cffff04ff02ffff04ff0dff80808080ff808080808080ffff
    0181dc80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c3
    85a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721
    e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd25
    31e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879
    b7152a6e7298a91ce119a63400ade7c5ffffffff04ff28ffff04ff05ffff04ff
    ff0101ffff04ffff04ff05ff8080ff8080808080ff04ff28ffff04ffff02ff2a
    ffff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04
    ff80ffff04ffff04ff17ff8080ff8080808080ffff0bff81bcffff02ff3affff
    04ff02ffff04ff05ffff04ffff02ff2cffff04ff02ffff04ff07ff80808080ff
    808080808080ff0bff34ffff0bff34ff81dcff0580ffff0bff34ff0bff819c80
    80ffffff02ffff03ff0bffff01ff02ffff03ffff22ffff09ffff12ff81e3ffff
    11ff83013bfdff81a38080ffff10ffff12ff8193ff8205fd80ff81d38080ffff
    15ff81d3ffff0181ff80ffff15ff8205fdff81d380ffff20ffff15ff81b3ff81
    e3808080ffff01ff04ffff02ff7effff04ff02ffff04ff82017dffff04ffff02
    ff2effff04ff02ffff04ff23ff80808080ff8080808080ffff04ffff02ff32ff
    ff04ff02ffff04ff82017dffff04ffff02ff2effff04ff02ffff04ffff04ff43
    ffff04ff83013bfdffff10ff81e3ff81b3808080ff80808080ffff04ff43ff80
    8080808080ffff04ffff04ffff0181d6ffff04ff28ffff04ff43ffff04ff8193
    ffff04ffff04ff43ff8080ff808080808080ffff02ff36ffff04ff02ffff04ff
    05ffff04ff23ffff04ff81f3ffff04ff81b3ffff04ffff02ff26ffff04ff02ff
    ff04ff05ffff04ff1bffff04ffff11ff17ff819380ffff04ffff11ff2fff81b3
    80ffff04ffff11ff5fff81d380ff8080808080808080ff808080808080808080
    8080ffff01ff088080ff0180ffff01ff21ff17ff5f8080ff0180ff02ffff03ff
    17ffff01ff04ffff04ff24ffff04ffff0117ffff04ffff02ff2effff04ff02ff
    ff04ffff04ffff0101ffff04ffff02ff22ffff04ff02ffff04ffff02ff2affff
    04ff02ffff04ff5dffff04ffff02ff2effff04ff02ffff04ffff04ff13ff81a7
    80ff80808080ffff04ff81bdff808080808080ff80808080ff808080ff808080
    80ffff04ffff30ff820167ffff02ff2affff04ff02ffff04ff11ffff04ffff02
    ff2effff04ff02ffff04ffff04ff11ffff04ff8202e7ff398080ff80808080ff
    ff04ffff02ff2affff04ff02ffff04ff15ffff04ffff0bffff0101ff1580ffff
    04ff8205e7ffff04ff820be7ffff04ffff02ff2affff04ff02ffff04ff2dffff
    04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff822fe780ffff04ff8217
    e7ffff04ffff02ff2affff04ff02ffff04ff5dffff04ffff02ff2effff04ff02
    ffff04ffff04ff13ffff11ff81a7ff478080ff80808080ffff04ff81bdff8080
    80808080ff8080808080808080ff8080808080808080ff808080808080ffff01
    0180ff8080808080ffff04ffff04ff38ffff04ffff0effff0172ff8202e780ff
    808080ffff02ffff03ffff22ffff09ff8227fdffff02ff5effff04ff02ffff04
    ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ff8202e7ff81a780ff
    8080808080ffff04ff823fe7ff808080808080ff4780ffff01ff02ff36ffff04
    ff02ffff04ff05ffff04ff0bffff04ff37ffff04ffff11ff2fff4780ffff04ff
    5fff8080808080808080ffff01ff088080ff01808080ffff01ff02ffff03ff2f
    ffff01ff0880ffff015f80ff018080ff0180ffff02ffff03ffff07ff0580ffff
    01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2e
    ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180
    ffff02ffff03ff1bffff01ff02ff5effff04ff02ffff04ffff02ffff03ffff18
    ffff0101ff1380ffff01ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05
    ff2b8080ff0180ffff04ffff04ffff17ff13ffff0181ff80ff3b80ff80808080
    80ffff010580ff0180ff04ff24ffff04ffff0112ffff04ff80ffff04ffff02ff
    2affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff80
    80808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    bb2f77da22a0165317788415dad4fbae4bcc603c1069e70845e76e3fb9425571
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

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorEntryPayoutInfo {
    pub payout_amount: u64,
    #[clvm(rest)]
    pub payout_rounding_error: u64,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotAndNfts {
    pub existing_slot_value: RewardDistributorEntrySlotValue,
    pub entry_payout_info: RewardDistributorEntryPayoutInfo,
    pub nfts_total_shares_delta: i64,
    #[clvm(rest)]
    pub nfts: Vec<RefreshNftInfo>,
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorDlInfo {
    pub dl_metadata_rest_hash: Option<Bytes32>,
    pub dl_metadata_updater_hash_hash: Bytes32,
    #[clvm(rest)]
    pub dl_inner_puzzle_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRefreshNftsTotals {
    pub total_entry_payout_amount: u64,
    pub total_shares_delta: i128,
    #[clvm(rest)]
    pub total_payout_rounding_error: u64,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRefreshNftsFromDlActionSolution {
    pub dl_root_hash: Bytes32,
    pub dl_info: RewardDistributorDlInfo,
    pub totals: RewardDistributorRefreshNftsTotals,
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
