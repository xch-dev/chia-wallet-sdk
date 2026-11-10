use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{MerkleProof, Mod, puzzles::RewardDistributorEntrySlotValue};

/// Serialized reward distributor refresh action for curated NFT distributors, compiled
/// from Rue.
///
/// When the curated `DataLayer` list changes an NFT's shares, this action updates staked
/// NFTs to match. For each affected entry, it pays out the entry's pending rewards,
/// re-creates the entry slot with the current cumulative payout and the adjusted shares,
/// and for each NFT proves its new shares against the store's root, replaces its deposit
/// slot, and re-locks the NFT. The action announces `"r"` followed by each refreshed
/// NFT's launcher id.
pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE: [u8; 1976] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ff8209ffffff04ffff11ff8215ffff824fff80ffff04
    ffff10ff822dffff82afff80ffff04ffff04ff829dffffff10ff82ddffff82ef
    ff8080ff827dff80808080ffff04ffff04ffff0155ffff04ffff10ff82bdffff
    82017f80ff808080ffff04ffff04ffff013fffff04ffff0bffff02ff0cffff04
    ff16ffff04ff09ffff04ffff02ff08ffff04ff08ff058080ffff04ffff02ff0c
    ffff04ff16ffff04ff0bffff04ffff0bffff0101ff0b80ffff04ffff0bffff01
    02ffff0bffff0101ff820bff80ffff02ffff03ff8227ffffff018227ffffff01
    ff0bffff01018080ff018080ffff04ff8257ffffff04ff8277ffff8080808080
    808080ff808080808080ffff012480ff808080ffff02ff12ffff04ff02ffff04
    ffff04ff03ff823fff80ff822fff808080808080ffff04ffff04ffff04ffff01
    ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff05
    8080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff01
    80ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff
    0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02
    ff078080ffff0bffff01018080808080ffff04ffff04ffff01ff02ffff03ff0d
    ffff01ff02ffff03ffff22ffff22ffff22ffff09ffff12ffff11ff830277f9ff
    82016580ff8201e580ffff10ffff12ff8195ff820bf980ff81d58080ffff15ff
    81d5ffff0181ff8080ffff15ff820bf9ff81d58080ffff20ffff15ff80ffff10
    ff8201e5ff81b580808080ffff01ff04ffff02ff2effff04ffff04ff0cff1680
    ffff04ff8202f9ffff02ff08ffff04ff08ff258080808080ffff04ffff02ff3a
    ffff04ffff04ff0cff1680ffff04ff8202f9ffff04ffff02ff08ffff04ff08ff
    ff04ffff10ff45ffff010180ffff04ff81a5ffff04ff830277f9ffff10ff8201
    e5ff81b5808080808080ff81a580808080ffff04ffff04ffff0181d6ffff04ff
    ff0133ffff04ff81a5ffff04ff8195ffff04ffff04ff81a5ff8080ff80808080
    8080ffff02ff2affff04ffff04ff08ffff04ffff04ff0cff2a80ffff04ffff04
    ff3aff1680ff1e808080ffff04ff09ffff04ff25ffff04ff81f5ffff04ff81b5
    ffff02ff12ffff04ff02ffff04ffff04ff09ff1d80ffff04ffff11ff0bff8195
    80ffff04ffff11ff17ff81b580ffff11ff1fff81d58080808080808080808080
    80808080ffff01ff088080ff0180ffff01ff02ffff03ffff22ffff22ffff20ff
    0b80ffff20ff1f8080ffff20ff178080ffff0180ffff01ff088080ff018080ff
    0180ffff04ffff01ff02ffff03ff17ffff01ff02ffff03ffff22ffff22ffff22
    ff47ffff09ff8217fdffff02ff3effff04ff3effff04ff823fe7ffff0bffff01
    01ffff02ff04ffff04ff04ffff04ff8202e7ff81a7808080808080808080ffff
    20ffff15ff80ff81a7808080ffff20ffff15ff80ffff11ff81a7ff4780808080
    ffff01ff04ffff04ffff0142ffff04ffff0117ffff04ffff02ff04ffff04ff04
    ffff04ffff0101ffff04ffff04ffff0133ffff04ff81bdffff04ffff0101ffff
    04ffff04ffff02ff04ffff04ff04ffff04ff2bff81bd808080ff8080ff808080
    8080ff8080808080ffff04ffff30ff820167ffff02ff12ffff04ff36ffff04ff
    11ffff04ffff02ff04ffff04ff04ffff04ff11ffff04ff8202e7ff3980808080
    ffff04ffff02ff12ffff04ff36ffff04ff15ffff04ffff0bffff0101ff1580ff
    ff04ff8205e7ffff04ff820be7ffff04ffff02ff12ffff04ff36ffff04ff2dff
    ff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff822fe780ffff04ff82
    17e7ffff04ff81bdff8080808080808080ff8080808080808080ff8080808080
    80ffff010180ff8080808080ffff04ffff04ffff013effff04ffff0effff0172
    ff8202e780ff808080ffff04ffff02ff2effff04ffff04ff12ff3680ffff04ff
    5dffff02ff04ffff04ff04ffff04ff2bffff04ffff11ff81a7ff4780ff8202e7
    80808080808080ffff04ffff02ff26ffff04ffff04ff12ff3680ffff04ff5dff
    ff04ffff02ff04ffff04ff04ffff04ff2bffff04ff81a7ff8202e780808080ff
    2b80808080ffff02ff1affff04ff02ffff04ff05ffff04ff0bffff04ff37ffff
    04ffff11ff2fff4780ff3f80808080808080808080ffff01ff088080ff0180ff
    ff01ff02ffff03ff2fffff01ff0880ffff013f80ff018080ff0180ffff01ff04
    ffff0133ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101
    ff0b80ff8080808080ffff04ff80ffff04ffff04ff0fff8080ff808080808080
    80ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff01820104
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff
    0bffff018201018080ff0180ffff04ffff01ff04ffff0142ffff04ffff0112ff
    ff04ff80ffff04ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101
    ff0780ff8080808080ff8080808080ffff01ff02ffff03ff0dffff01ff02ff02
    ffff04ff02ffff04ffff04ffff17ff09ffff0181ff80ff1d80ffff0bffff0102
    ffff03ffff18ff09ffff010180ff15ff0780ffff03ffff18ff09ffff010180ff
    07ff158080808080ffff010780ff018080808080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE`].
pub const REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    831dc0668f5b8b573c869cfa08dddb91078ad1792ce3276bd531e6eaf4577dff
    "
));

/// Curried arguments of the refresh action ([`REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRefreshNftsFromDlActionArgs {
    /// Singleton struct of the `DataLayer` store that holds the curated list.
    pub dl_singleton_struct: SingletonStruct,
    /// Mod hash of the NFT state layer.
    pub nft_state_layer_mod_hash: Bytes32,
    /// Mod hash of the NFT ownership layer.
    pub nft_ownership_layer_mod_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for deposit slots
    /// ([`RewardDistributorSlotNonce::DEPOSIT`](crate::puzzles::RewardDistributorSlotNonce::DEPOSIT)).
    pub deposit_slot_1st_curry_hash: Bytes32,
    /// Puzzle hash that holds staked assets: [`P2DelegatedBySingletonLayerArgs`](crate::puzzles::P2DelegatedBySingletonLayerArgs)
    /// for the reward distributor singleton, with nonce 1.
    pub my_p2_puzzle_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for entry slots
    /// ([`RewardDistributorSlotNonce::ENTRY`](crate::puzzles::RewardDistributorSlotNonce::ENTRY)).
    pub entry_slot_1st_curry_hash: Bytes32,
    /// Maximum number of seconds since the distributor's last update. The action asserts
    /// that the spend happens before `last_update + max_second_offset`, so entries can't be
    /// changed with stale reward accounting.
    pub max_second_offset: u64,
    /// Fixed-point precision of the distributor's cumulative payout and remaining rewards.
    pub precision: u64,
}

impl RewardDistributorRefreshNftsFromDlActionArgs {
    /// Creates the arguments with the standard NFT mod hashes.
    pub fn new(
        dl_launcher_id: Bytes32,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
        entry_slot_1st_curry_hash: Bytes32,
        max_second_offset: u64,
        precision: u64,
    ) -> Self {
        Self {
            dl_singleton_struct: SingletonStruct::new(dl_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            deposit_slot_1st_curry_hash,
            my_p2_puzzle_hash,
            entry_slot_1st_curry_hash,
            max_second_offset,
            precision,
        }
    }
}

/// Information about one staked NFT being refreshed.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RefreshNftInfo {
    /// Change in the NFT's shares: `new_nft_shares` minus the shares in its current deposit
    /// slot.
    pub nft_shares_delta: i64,
    /// Shares the NFT is worth according to the store.
    pub new_nft_shares: u64,
    /// Parent coin id of the locked NFT coin.
    pub nft_parent_id: Bytes32,
    /// Launcher id of the NFT.
    pub nft_launcher_id: Bytes32,
    /// Tree hash of the NFT's metadata.
    pub nft_metadata_hash: Bytes32,
    /// Tree hash of the NFT's metadata updater puzzle hash.
    pub nft_metadata_updater_hash_hash: Bytes32,
    /// Puzzle hash of the NFT's royalty transfer program.
    pub nft_transfer_porgram_hash: Bytes32,
    /// Current owner (DID launcher id) of the NFT, if any.
    pub nft_owner: Option<Bytes32>,
    /// Proof that `(nft_launcher_id . new_nft_shares)` is in the store's Merkle root.
    #[clvm(rest)]
    pub nft_inclusion_proof: MerkleProof,
}

/// Payout owed to an entry, encoded as `(payout_amount . payout_rounding_error)`.
///
/// `payout_amount` is `shares * (cumulative_payout - initial_cumulative_payout) / precision`
/// and `payout_rounding_error` is the remainder, which goes back to the remaining
/// rewards.
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorEntryPayoutInfo {
    /// Amount paid to the entry's payout puzzle hash.
    pub payout_amount: u64,
    /// Remainder of the division by the precision.
    #[clvm(rest)]
    pub payout_rounding_error: u128,
}

/// An entry being refreshed and its NFTs.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotAndNfts {
    /// Current value of the entry's slot.
    pub existing_slot_value: RewardDistributorEntrySlotValue,
    /// Pending rewards paid to the entry.
    pub entry_payout_info: RewardDistributorEntryPayoutInfo,
    /// Sum of the NFTs' [`nft_shares_delta`](RefreshNftInfo::nft_shares_delta).
    pub nfts_total_shares_delta: i64,
    /// NFTs of the entry being refreshed.
    #[clvm(rest)]
    pub nfts: Vec<RefreshNftInfo>,
}

/// Information used to reconstruct the curated list's `DataLayer` store puzzle.
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorDlInfo {
    /// Tree hash of the store's metadata after the root hash.
    pub dl_metadata_rest_hash: Option<Bytes32>,
    /// Tree hash of the store's metadata updater puzzle hash.
    pub dl_metadata_updater_hash_hash: Bytes32,
    /// Puzzle hash of the store's inner puzzle below the NFT state layer (its delegation
    /// layer).
    #[clvm(rest)]
    pub dl_inner_puzzle_hash: Bytes32,
}

/// Totals over all refreshed entries.
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRefreshNftsTotals {
    /// Sum of the entries' payout amounts.
    pub total_entry_payout_amount: u64,
    /// Sum of the entries' share changes.
    pub total_shares_delta: i128,
    /// Sum of the entries' payout rounding errors.
    #[clvm(rest)]
    pub total_payout_rounding_error: u128,
}

/// Solution to the refresh action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRefreshNftsFromDlActionSolution {
    /// Current Merkle root of the `DataLayer` store.
    pub dl_root_hash: Bytes32,
    /// Information used to reconstruct the store's puzzle.
    pub dl_info: RewardDistributorDlInfo,
    /// Totals over all refreshed entries.
    pub totals: RewardDistributorRefreshNftsTotals,
    /// Entries to refresh and their NFTs.
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
