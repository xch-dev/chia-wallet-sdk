use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{MerkleProof, Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE: [u8; 1156] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff2effff04ff16
    ff3e80808080ffff04ff03ffff04ff820bffffff01ff808080808080ffff
    04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02
    ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff01
    01ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff
    0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102
    ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff
    02ffff03ff0bffff01ff02ffff01ff02ffff03ffff09ff825ffbffff02ff7dff
    ff04ff7dffff04ff820fe7ffff0bffff0101ffff02ff09ffff04ff09ffff04ff
    47ff820be78080808080808080ffff01ff02ff5dffff04ff05ffff04ff0bffff
    04ff37ffff04ffff10ff2fff820be780ffff04ffff04ffff013fffff04ff02ff
    808080ffff04ffff04ffff013effff04ffff0effff016cff0280ff808080ff3f
    80808080808080ffff01ff088080ff0180ffff04ffff0bffff02ff0affff04ff
    16ffff04ff11ffff04ffff02ff04ffff04ff04ffff04ff11ffff04ff23ff3980
    808080ffff04ffff02ff0affff04ff16ffff04ff15ffff04ffff0bffff0101ff
    1580ffff04ff53ffff04ff81b3ffff04ffff02ff0affff04ff16ffff04ff2dff
    ff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff82017380ffff04ff82
    02f3ffff04ff5dff8080808080808080ff8080808080808080ff808080808080
    ffff02ff04ffff04ff04ffff04ffff02ff04ffff04ff04ffff04ff17ffff04ff
    8202fdff820bfd80808080ffff04ffff04ffff02ff0affff04ff16ffff04ff81
    bdffff04ffff02ff04ffff04ff04ffff04ff8205fdff8205f3808080ffff04ff
    82017dff808080808080ffff04ffff0101ffff04ffff04ffff02ff04ffff04ff
    04ffff04ff8205fdff82017d808080ff8080ff80808080ff808080808080ff01
    8080ffff01ff04ff17ffff04ffff04ffff013fffff04ffff0bffff02ff0affff
    04ff16ffff04ff11ffff04ffff02ff04ffff04ff04ff098080ffff04ffff02ff
    0affff04ff16ffff04ff15ffff04ffff0bffff0101ff1580ffff04ffff0bffff
    0102ffff0bffff0101ff822ffd80ffff02ffff03ff825ffdffff01825ffdffff
    01ff0bffff01018080ff018080ffff04ff82bffdffff04ff82fffdff80808080
    80808080ff808080808080ffff012480ff808080ffff04ffff04ffff0146ffff
    04ff820bfdff808080ff1f80808080ff0180ffff04ffff01ff02ffff03ff03ff
    ff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102
    ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff07
    8080ffff0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01
    ff02ffff03ff0dffff01ff02ff02ffff04ff02ffff04ffff04ffff17ff09ffff
    0181ff80ff1d80ffff0bffff0102ffff03ffff18ff09ffff010180ff15ff0780
    ffff03ffff18ff09ffff010180ff07ff158080808080ffff010780ff01808080
    8080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    40cd1303aad4496bb53e05b826669ecdf8ef0139bada26d75d96b0279e443b56
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
