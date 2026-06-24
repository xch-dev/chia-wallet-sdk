use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, MerkleProof, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE: [u8; 1157] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff2effff04ff16
    ff3e80808080ffff04ff03ffff04ff820bffffff04ff80ff808080808080ffff
    04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02
    ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff01
    01ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff
    0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102
    ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff
    02ffff03ff0bffff01ff02ffff01ff02ffff01ff02ffff03ffff09ff82bff7ff
    ff02ff81fbffff04ff81fbffff04ff821fcfffff0bffff0101ffff02ff13ffff
    04ff13ffff04ff818fff8217cf8080808080808080ffff01ff02ff81bbffff04
    ff0bffff04ff17ffff04ff6fffff04ffff10ff5fff8217cf80ffff04ffff04ff
    ff013fffff04ff02ff808080ffff04ffff04ffff013effff04ffff0effff016c
    ff0280ff808080ff7f80808080808080ffff01ff088080ff0180ffff04ffff0b
    ffff02ff15ffff04ff2dffff04ff23ffff04ffff02ff09ffff04ff09ffff04ff
    23ffff04ff47ff7380808080ffff04ffff02ff15ffff04ff2dffff04ff2bffff
    04ffff0bffff0101ff2b80ffff04ff81a7ffff04ff820167ffff04ffff02ff15
    ffff04ff2dffff04ff5bffff04ffff0bffff0101ff5b80ffff04ffff0bffff01
    01ff8202e780ffff04ff8205e7ffff04ff81bbff8080808080808080ff808080
    8080808080ff808080808080ffff02ff09ffff04ff09ffff04ffff02ff09ffff
    04ff09ffff04ff2fffff04ff8205fbff8217fb80808080ffff04ffff04ff02ff
    ff04ffff0101ffff04ffff04ff02ff8080ff80808080ff808080808080ff0180
    80ffff04ffff02ff0affff04ff16ffff04ff81bdffff04ffff02ff04ffff04ff
    04ffff04ff8205fdff8205f3808080ffff04ff82017dff808080808080ff0180
    80ffff01ff04ff17ffff04ffff04ffff013fffff04ffff0bffff02ff0affff04
    ff16ffff04ff11ffff04ffff02ff04ffff04ff04ff098080ffff04ffff02ff0a
    ffff04ff16ffff04ff15ffff04ffff0bffff0101ff1580ffff04ffff0bffff01
    02ffff0bffff0101ff822ffd80ffff02ffff03ff825ffdffff01825ffdffff01
    ff0bffff01018080ff018080ffff04ff82bffdffff04ff82fffdff8080808080
    808080ff808080808080ffff012480ff808080ffff04ffff04ffff0146ffff04
    ff820bfdff808080ff1f80808080ff0180ffff04ffff01ff02ffff03ff03ffff
    01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ff
    ff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff0780
    80ffff0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff
    02ffff03ff0dffff01ff02ff02ffff04ff02ffff04ffff04ffff17ff09ffff01
    81ff80ff1d80ffff0bffff0102ffff03ffff18ff09ffff010180ff15ff0780ff
    ff03ffff18ff09ffff010180ff07ff158080808080ffff010780ff0180808080
    80ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b05dfa1c88c19507af40251e0b120b2a77f0f2a96794958dc33074f63967a2ec
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsFromDlLockingPuzzleArgs {
    pub dl_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl RewardDistributorNftsFromDlLockingPuzzleArgs {
    pub fn new(store_launcher_id: Bytes32, my_p2_puzzle_hash: Bytes32) -> Self {
        Self {
            dl_singleton_struct: SingletonStruct::new(store_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StakeNftFromDlInfo {
    pub nft_launcher_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_owner: Option<Bytes32>,
    pub nft_transfer_porgram_hash: Bytes32,
    pub nft_shares: u64,
    #[clvm(rest)]
    pub nft_inclusion_proof: MerkleProof,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNftsFromDlLockingPuzzleSolution {
    pub my_id: Bytes32,
    pub nft_infos: Vec<StakeNftFromDlInfo>,
    pub dl_root_hash: Bytes32,
    pub dl_metadata_rest_hash: Option<Bytes32>,
    pub dl_metadata_updater_hash_hash: Bytes32,
    #[clvm(rest)]
    pub dl_inner_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorNftsFromDlLockingPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH
    }
}
