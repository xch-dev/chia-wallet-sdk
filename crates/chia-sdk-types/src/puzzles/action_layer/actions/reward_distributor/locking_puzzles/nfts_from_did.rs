use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{CompactLineageProof, NONCE_WRAPPER_PUZZLE_HASH},
};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE: [u8; 982] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff2effff04ff3e
    ff1680808080ffff04ff03ffff04ff8207ffffff01ff808080808080ffff
    04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02
    ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff01
    01ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff
    0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102
    ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff
    02ffff03ff0bffff01ff02ffff01ff02ff7dffff04ff05ffff04ff0bffff04ff
    37ffff04ffff10ff2fffff010180ffff04ffff04ffff013fffff04ff02ff8080
    80ffff04ffff04ffff013effff04ffff0effff016cff0280ff808080ff3f8080
    8080808080ffff04ffff0bffff02ff0affff04ff16ffff04ff11ffff04ffff02
    ff04ffff04ff04ffff04ff11ffff04ffff02ff2effff04ff2effff04ff8203f3
    ffff04ffff30ff8204f3ffff02ff0affff04ff16ffff04ff11ffff04ffff02ff
    04ffff04ff04ff098080ffff04ff820af3ff808080808080ff820ef380ffff04
    ff39ff808080808080ff3980808080ffff04ffff02ff0affff04ff16ffff04ff
    15ffff04ffff0bffff0101ff1580ffff04ff23ffff04ff53ffff04ffff02ff0a
    ffff04ff16ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff01
    01ff81b380ffff04ff820173ffff04ff5dff8080808080808080ff8080808080
    808080ff808080808080ffff02ff04ffff04ff04ffff04ffff02ff04ffff04ff
    04ffff04ff17ffff04ff8202fdff820bfd80808080ffff04ffff04ffff02ff0a
    ffff04ff16ffff04ff81bdffff04ffff02ff04ffff04ff04ffff04ff8205fdff
    ff0101808080ffff04ff82017dff808080808080ffff04ffff0101ffff04ffff
    04ffff02ff04ffff04ff04ffff04ff8205fdff82017d808080ff8080ff808080
    80ff808080808080ff018080ffff01ff04ff17ffff04ffff04ffff0146ffff04
    ff820bfdff808080ff1f808080ff0180ffff04ffff01ff02ffff03ff03ffff01
    ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff
    0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080
    ffff0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff02
    ffff03ff05ffff01ff30ffff02ff02ffff04ff02ffff04ff0dffff04ff0bffff
    04ff17ffff01018080808080ff11ffff03ffff22ff1fffff09ff11ff178080ff
    ff0181ffff198080ffff010b80ff018080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d7358112fd4b1c1f384c150eb56e70cce2ae2108a510e0a184c413ad69b32363
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsFromDidLockingPuzzleArgs {
    pub did_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl RewardDistributorNftsFromDidLockingPuzzleArgs {
    pub fn new(did_launcher_id: Bytes32, my_p2_puzzle_hash: Bytes32) -> Self {
        Self {
            did_singleton_struct: SingletonStruct::new(did_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash,
        }
    }
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct IntermediaryCoinProof {
    pub full_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub amount: u64,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct NftLauncherProof {
    pub did_proof: CompactLineageProof,
    #[clvm(rest)]
    pub intermediary_coin_proofs: Vec<IntermediaryCoinProof>,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StakeNftFromDidInfo {
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_owner: Option<Bytes32>,
    pub nft_transfer_porgram_hash: Bytes32,
    #[clvm(rest)]
    pub nft_launcher_proof: NftLauncherProof,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNftsFromDidLockingPuzzleSolution {
    pub my_id: Bytes32,
    #[clvm(rest)]
    pub nft_infos: Vec<StakeNftFromDidInfo>,
}

impl Mod for RewardDistributorNftsFromDidLockingPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH
    }
}
