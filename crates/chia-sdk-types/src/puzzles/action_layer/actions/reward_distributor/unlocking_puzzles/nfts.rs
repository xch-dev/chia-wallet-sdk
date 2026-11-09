use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized reward distributor unlock puzzle for staked NFTs, compiled from Rue.
///
/// For each NFT, spends its deposit slot and approves a delegated puzzle that sends the
/// NFT back to the custody puzzle hash, which must request the unstake with a message of
/// `"u"` followed by the NFT's launcher id. Returns the total shares removed.
pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE: [u8; 837] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff1eff16808080
    ffff04ff03ffff04ff8203ffffff01ff808080808080ffff04ffff04ffff01ff
    02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff0580
    80ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180
    ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0b
    ffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04
    ff02ff078080ffff0bffff010180808080ffff04ffff01ff02ffff03ff0bffff
    01ff02ffff03ffff15ff80ff8207f380ffff01ff0880ffff01ff02ff1effff04
    ff02ffff04ff05ffff04ff1bffff04ffff10ff17ff8207f380ffff04ffff04ff
    ff0142ffff04ffff0117ffff04ffff02ff04ffff04ff04ffff04ffff0101ffff
    04ffff04ffff0133ffff04ff8205fdffff04ffff0101ffff04ffff04ff8205fd
    ff8080ff8080808080ffff01ffff52ff80808080808080ffff04ffff30ff53ff
    ff02ff0affff04ff16ffff04ff09ffff04ffff02ff04ffff04ff04ffff04ff09
    ffff04ff23ff1580808080ffff04ffff02ff0affff04ff16ffff04ff2dffff04
    ffff0bffff0101ff2d80ffff04ff81b3ffff04ff820173ffff04ffff02ff0aff
    ff04ff16ffff04ff5dffff04ffff0bffff0101ff5d80ffff04ffff0bffff0101
    ff8202f380ffff04ff8205f3ffff04ff82017dff8080808080808080ff808080
    8080808080ff808080808080ffff010180ff8080808080ffff04ffff04ffff01
    43ffff04ffff0112ffff04ffff0effff0175ff2380ffff04ff8205fdff808080
    8080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff0a
    ffff04ff16ffff04ff81bdffff04ffff0bffff0101ffff02ff04ffff04ff04ff
    ff04ff8205fdffff04ff8207f3ff238080808080ff8080808080ff8080808080
    ff1f808080808080808080ff0180ffff010f80ff0180ffff01ff02ffff03ff03
    ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    078080ffff0bffff010180808080ffff01ff0bffff018201018080ff01808080
    80ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE`].
pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3baf9c5ee72391dda01d40243db292668665a93e5ad5a6433c45bfda6646aed2
    "
));

/// Curried arguments of [`REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsUnlockingPuzzleArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Puzzle hash of the singleton launcher.
    pub singleton_launcher_hash: Bytes32,
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
}

impl RewardDistributorNftsUnlockingPuzzleArgs {
    /// Creates the arguments with the standard singleton and NFT mod hashes.
    pub fn new(my_p2_puzzle_hash: Bytes32, deposit_slot_1st_curry_hash: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_hash: SINGLETON_LAUNCHER_HASH.into(),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            deposit_slot_1st_curry_hash,
            my_p2_puzzle_hash,
        }
    }
}

/// Information about a staked NFT to unlock.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NftToUnlockInfo {
    /// Launcher id of the NFT.
    pub nft_launcher_id: Bytes32,
    /// Parent coin id of the locked NFT coin.
    pub nft_parent_id: Bytes32,
    /// Tree hash of the NFT's metadata.
    pub nft_metadata_hash: Bytes32,
    /// Tree hash of the NFT's metadata updater puzzle hash.
    pub nft_metadata_updater_hash_hash: Bytes32,
    /// Current owner (DID launcher id) of the NFT, if any.
    pub nft_owner: Option<Bytes32>,
    /// Puzzle hash of the NFT's royalty transfer program.
    pub nft_transfer_porgram_hash: Bytes32,
    /// Shares recorded in the NFT's deposit slot.
    #[clvm(rest)]
    pub nft_shares: u64,
}

/// Solution to [`REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE`], after the ephemeral state and
/// custody puzzle hash: the NFTs to unlock.
pub type RewardDistributorNftsUnlockingPuzzleSolution = Vec<NftToUnlockInfo>;

impl Mod for RewardDistributorNftsUnlockingPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH
    }
}
