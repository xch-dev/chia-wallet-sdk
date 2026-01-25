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

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE: [u8; 1985] = hex!(
    "
    ff02ffff01ff04ffff04ff8209ffffff04ffff11ff8215ffff824fff80ffff04
    ffff10ff822dffff82afff80ffff04ffff04ff829dffffff10ff82ddffff82ef
    ff8080ff827dff80808080ffff04ffff04ff20ffff04ffff10ff82bdffff8201
    7f80ff808080ffff04ffff04ff30ffff04ffff0bffff02ff2affff04ff02ffff
    04ff09ffff04ffff02ff2effff04ff02ffff04ff05ff80808080ffff04ffff02
    ff2affff04ff02ffff04ff0bffff04ffff0bffff0101ff0b80ffff04ffff0bff
    ff0102ffff0bffff0101ff820bff80ffff02ffff03ff8227ffffff018227ffff
    ff01819c80ff018080ffff04ff8257ffffff04ff8277ffff8080808080808080
    ff808080808080ffff012480ff808080ffff02ff26ffff04ff02ffff04ff03ff
    ff04ff823fffffff04ff824fffffff04ff82afffffff04ff82efffff80808080
    80808080808080ffff04ffff01ffffffff553fff333effff4202ffff02ffff03
    ff05ffff01ff0bff81fcffff02ff3affff04ff02ffff04ff09ffff04ffff02ff
    2cffff04ff02ffff04ff0dff80808080ff808080808080ffff0181dc80ff0180
    ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c77
    85459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596
    718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225
    f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a9
    1ce119a63400ade7c5ffffffff04ff28ffff04ff05ffff04ffff0101ffff04ff
    ff04ff05ff8080ff8080808080ff04ff28ffff04ffff02ff2affff04ff02ffff
    04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff
    04ff17ff8080ff8080808080ffff0bff81bcffff02ff3affff04ff02ffff04ff
    05ffff04ffff02ff2cffff04ff02ffff04ff07ff80808080ff808080808080ff
    0bff34ffff0bff34ff81dcff0580ffff0bff34ff0bff819c8080ffffff02ffff
    03ff0bffff01ff02ffff03ffff22ffff09ffff12ff81e3ffff11ff83013bfdff
    81a38080ffff10ffff12ff8193ff8205fd80ff81d38080ffff15ff81d3ffff01
    81ff80ffff15ff8205fdff81d380ffff20ffff15ff81b3ff81e3808080ffff01
    ff04ffff02ff7effff04ff02ffff04ff82017dffff04ffff02ff2effff04ff02
    ffff04ff23ff80808080ff8080808080ffff04ffff02ff32ffff04ff02ffff04
    ff82017dffff04ffff02ff2effff04ff02ffff04ffff04ff43ffff04ff83013b
    fdffff10ff81e3ff81b3808080ff80808080ffff04ff43ff808080808080ffff
    04ffff04ffff0181d6ffff04ff28ffff04ff43ffff04ff8193ffff04ffff04ff
    43ff8080ff808080808080ffff02ff36ffff04ff02ffff04ff05ffff04ff23ff
    ff04ff81f3ffff04ff81b3ffff04ffff02ff26ffff04ff02ffff04ff05ffff04
    ff1bffff04ffff11ff17ff819380ffff04ffff11ff2fff81b380ffff04ffff11
    ff5fff81d380ff8080808080808080ff8080808080808080808080ffff01ff08
    8080ff0180ffff01ff21ff17ff5f8080ff0180ff02ffff03ff17ffff01ff04ff
    ff04ff24ffff04ffff0117ffff04ffff02ff2effff04ff02ffff04ffff04ffff
    0101ffff04ffff02ff22ffff04ff02ffff04ffff02ff2affff04ff02ffff04ff
    5dffff04ffff02ff2effff04ff02ffff04ffff04ff13ff81a780ff80808080ff
    ff04ff81bdff808080808080ff80808080ff808080ff80808080ffff04ffff30
    ff820167ffff02ff2affff04ff02ffff04ff11ffff04ffff02ff2effff04ff02
    ffff04ffff04ff11ffff04ff8202e7ff398080ff80808080ffff04ffff02ff2a
    ffff04ff02ffff04ff15ffff04ffff0bffff0101ff1580ffff04ff8205e7ffff
    04ff820be7ffff04ffff02ff2affff04ff02ffff04ff2dffff04ffff0bffff01
    01ff2d80ffff04ffff0bffff0101ff822fe780ffff04ff8217e7ffff04ffff02
    ff2affff04ff02ffff04ff5dffff04ffff02ff2effff04ff02ffff04ffff04ff
    13ffff11ff81a7ff478080ff80808080ffff04ff81bdff808080808080ff8080
    808080808080ff8080808080808080ff808080808080ffff010180ff80808080
    80ffff04ffff04ff38ffff04ffff0effff0172ff8202e780ff808080ffff02ff
    ff03ffff22ffff09ff8217fdffff02ff5effff04ff02ffff04ffff0bffff0101
    ffff02ff2effff04ff02ffff04ffff04ff8202e7ff81a780ff8080808080ffff
    04ff823fe7ff808080808080ff4780ffff01ff02ff36ffff04ff02ffff04ff05
    ffff04ff0bffff04ff37ffff04ffff11ff2fff4780ffff04ff5fff8080808080
    808080ffff01ff088080ff01808080ffff01ff02ffff03ff2fffff01ff0880ff
    ff015f80ff018080ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ffff02ffff03ff
    1bffff01ff02ff5effff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380
    ffff01ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180
    ffff04ffff04ffff17ff13ffff0181ff80ff3b80ff8080808080ffff010580ff
    0180ff04ff24ffff04ffff0112ffff04ff80ffff04ffff02ff2affff04ff02ff
    ff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff0180
    80
    "
);

pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    77a286dd18a66c17c7e8d60b8cfb4be187d1b6bf0e66fccd6915f340096ff565
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
    pub payout_rounding_error: u128,
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
    pub total_payout_rounding_error: u128,
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
