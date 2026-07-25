use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE: [u8; 767] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff1eff16808080
    ffff04ff03ffff04ff8203ffffff01ff808080808080ffff04ffff04ffff
    01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff
    058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff
    0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ff
    ff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ff
    ff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff02ffff03ff0b
    ffff01ff02ff1effff04ff02ffff04ff05ffff04ff1bffff04ffff10ff17ff82
    07f380ffff04ffff04ffff0142ffff04ffff0117ffff04ffff02ff04ffff04ff
    04ffff04ffff0101ffff04ffff04ffff0133ffff04ff8205fdffff04ffff0101
    ffff04ffff04ff8205fdff8080ff8080808080ffff01ffff52ff80
    808080808080ffff04ffff30ff53ffff02ff0affff04ff16ff
    ff04ff09ffff04ffff02ff04ffff04ff04ffff04ff09ffff04ff23ff15808080
    80ffff04ffff02ff0affff04ff16ffff04ff2dffff04ffff0bffff0101ff2d80
    ffff04ff81b3ffff04ff820173ffff04ffff02ff0affff04ff16ffff04ff5dff
    ff04ffff0bffff0101ff5d80ffff04ffff0bffff0101ff8202f380ffff04ff82
    05f3ffff04ffff02ff0affff04ff16ffff04ff81bdffff04ffff02ff04ffff04
    ff04ffff04ff8205fdff8207f3808080ffff04ff82017dff808080808080ff80
    80808080808080ff8080808080808080ff808080808080ffff010180ff808080
    8080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0175ff2380
    ffff04ff8205fdff8080808080ff1f80808080808080ffff010f80ff0180ffff
    01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201
    018080ff0180808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    134966b5b5f1930f23c3cf5b3e3e0a62a58dde426b4b76526d5c0743b8aade78
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsUnlockingPuzzleArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_hash: Bytes32,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl RewardDistributorNftsUnlockingPuzzleArgs {
    pub fn new(my_p2_puzzle_hash: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_hash: SINGLETON_LAUNCHER_HASH.into(),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct NftToUnlockInfo {
    pub nft_launcher_id: Bytes32,
    pub nft_parent_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_owner: Option<Bytes32>,
    pub nft_transfer_porgram_hash: Bytes32,
    #[clvm(rest)]
    pub nft_shares: u64,
}

pub type RewardDistributorNftsUnlockingPuzzleSolution = Vec<NftToUnlockInfo>;

impl Mod for RewardDistributorNftsUnlockingPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH
    }
}
