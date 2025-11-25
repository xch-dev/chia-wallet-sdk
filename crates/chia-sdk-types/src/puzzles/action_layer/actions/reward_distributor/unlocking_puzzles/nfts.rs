use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE: [u8; 873] = hex!(
    "
    ff02ffff01ff02ff2effff04ff02ffff04ff03ffff04ff8203ffffff01ff80ff
    808080808080ffff04ffff01ffffff3343ff42ff02ff02ffff03ff05ffff01ff
    0bff72ffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff02ff
    ff04ff0dff80808080ff808080808080ffff016280ff0180ffffffffa04bf512
    2f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf
    97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa1
    02a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f632
    22a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400
    ade7c5ff0bff52ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff3cffff
    04ff02ffff04ff07ff80808080ff808080808080ffff0bff2cffff0bff2cff62
    ff0580ffff0bff2cff0bff428080ffff02ffff03ff0bffff01ff02ff2effff04
    ff02ffff04ff05ffff04ff1bffff04ffff10ff17ff8207f380ffff04ffff04ff
    ff04ff14ffff04ffff0117ffff04ffff02ff3effff04ff02ffff04ffff04ffff
    0101ffff04ffff04ff10ffff04ff8205fdffff04ffff0101ffff04ffff04ff82
    05fdff8080ff8080808080ff808080ff80808080ffff04ffff30ff53ffff02ff
    1affff04ff02ffff04ff09ffff04ffff02ff3effff04ff02ffff04ffff04ff09
    ffff04ff23ff158080ff80808080ffff04ffff02ff1affff04ff02ffff04ff2d
    ffff04ffff0bffff0101ff2d80ffff04ff81b3ffff04ff820173ffff04ffff02
    ff1affff04ff02ffff04ff5dffff04ffff0bffff0101ff5d80ffff04ffff0bff
    ff0101ff8202f380ffff04ff8205f3ffff04ffff02ff1affff04ff02ffff04ff
    81bdffff04ffff02ff3effff04ff02ffff04ffff04ff8205fdff8207f380ff80
    808080ffff04ff82017dff808080808080ff8080808080808080ff8080808080
    808080ff808080808080ffff010180ff8080808080ffff04ffff04ff18ffff04
    ffff0112ffff04ffff0effff0175ff2380ffff04ff8205fdff8080808080ff2f
    8080ff80808080808080ffff01ff04ff17ff2f8080ff0180ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080
    ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a515fc039812314bdbf490b940dc3de5e07a1165463c4bd2f647d4d21e628312
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
