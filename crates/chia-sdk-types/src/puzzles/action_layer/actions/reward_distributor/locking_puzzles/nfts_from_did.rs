use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::CompactLineageProof};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE: [u8; 1049] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff2effff04ff3e
    ff1680808080ffff04ff03ffff04ff8207ffffff01ff808080808080ffff04ff
    ff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff
    04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff
    038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bff
    ff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff
    02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff02ff
    ff03ff0bffff01ff02ffff01ff02ffff01ff02ff81fbffff04ff0bffff04ff17
    ffff04ff6fffff04ffff10ff5fffff010180ffff04ffff04ffff0133ffff04ff
    ff02ff2bffff04ff5bffff04ff8202f7ffff04ffff0bffff0101ffff02ff13ff
    ff04ff13ffff04ff8217f7ffff04ffff0101ff058080808080ff8080808080ff
    ff04ff80ffff04ffff04ff8217f7ff8080ff8080808080ffff04ffff04ffff01
    3fffff04ff02ff808080ffff04ffff04ffff013effff04ffff0effff016cff02
    80ff808080ff7f8080808080808080ffff04ffff0bffff02ff15ffff04ff2dff
    ff04ff23ffff04ffff02ff09ffff04ff09ffff04ff23ffff04ff02ff73808080
    80ffff04ffff02ff15ffff04ff2dffff04ff2bffff04ffff0bffff0101ff2b80
    ffff04ff47ffff04ff81a7ffff04ffff02ff15ffff04ff2dffff04ff5bffff04
    ffff0bffff0101ff5b80ffff04ffff0bffff0101ff82016780ffff04ff8202e7
    ffff04ff81bbff8080808080808080ff8080808080808080ff808080808080ff
    ff02ff09ffff04ff09ffff04ffff02ff09ffff04ff09ffff04ff2fffff04ff82
    05fbff8217fb80808080ffff04ffff04ff8202fbffff04ffff0101ffff04ffff
    04ffff02ff09ffff04ff09ffff04ff820bfbff8202fb808080ff8080ff808080
    80ff808080808080ff018080ffff04ffff02ff2effff04ff2effff04ff8203f3
    ffff04ffff30ff8204f3ffff02ff0affff04ff16ffff04ff11ffff04ffff02ff
    04ffff04ff04ff098080ffff04ff820af3ff808080808080ff820ef380ffff04
    ff39ff808080808080ff018080ffff01ff04ff17ffff04ffff04ffff0146ffff
    04ff820bfdff808080ff1f808080ff0180ffff04ffff01ff02ffff03ff03ffff
    01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ff
    ff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff0780
    80ffff0bffff010180808080ffff01ff0bffff018201018080ff0180ffff01ff
    02ffff03ff05ffff01ff30ffff02ff02ffff04ff02ffff04ff0dffff04ff0bff
    ff04ff17ffff01018080808080ff11ffff03ffff22ff1fffff09ff11ff178080
    ffff0181ffff198080ffff010b80ff018080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    65a9dd0fed6d55f59a7a4adb6f33474fc69f43afa1565e465ed7512038f24c0a
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsFromDidLockingPuzzleArgs {
    pub did_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub deposit_slot_1st_curry_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
}

impl RewardDistributorNftsFromDidLockingPuzzleArgs {
    pub fn new(
        did_launcher_id: Bytes32,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
    ) -> Self {
        Self {
            did_singleton_struct: SingletonStruct::new(did_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            deposit_slot_1st_curry_hash,
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
