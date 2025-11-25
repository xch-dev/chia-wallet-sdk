use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE: [u8; 1652] = hex!(
    "
    ff02ffff01ff02ff3cffff04ff02ffff04ff03ffff04ff8207ffffff01ff80ff808080808080ffff04ffff01ffffff46ff3f3effff0102ffff04ffff04ff28ffff04ff05ff808080ffff04ffff04ff38ffff04ff05ff808080ff0b8080ff02ffff03ff0bffff01ff02ff3cffff04ff02ffff04ff05ffff04ff1bffff04ffff10ff17ffff010180ffff04ffff02ff2cffff04ff02ffff04ffff0bffff02ff3affff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ffff04ff11ffff04ffff02ff36ffff04ff02ffff04ffff30ff8204f3ffff02ff3affff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ff09ff80808080ffff04ff820af3ff808080808080ff8216f380ffff04ff8203f3ff8080808080ff398080ff80808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bffff0101ff1580ffff04ff23ffff04ff53ffff04ffff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380ffff04ff820173ffff04ff5dff8080808080808080ff8080808080808080ff808080808080ffff02ff3effff04ff02ffff04ffff04ffff02ff3effff04ff02ffff04ffff04ff17ffff04ff8202fdff820bfd8080ff80808080ffff04ffff02ff2effff04ff02ffff04ffff02ff3affff04ff02ffff04ff81bdffff04ffff02ff3effff04ff02ffff04ffff04ff8205fdffff010180ff80808080ffff04ff82017dff808080808080ff80808080ff808080ff8080808080ffff04ffff04ffff04ff24ffff04ff5dffff04ffff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380ffff04ff820173ffff04ff5dff8080808080808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bffff0101ff1580ffff04ff23ffff04ff53ffff04ffff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380ffff04ff820173ffff04ff5dff8080808080808080ff8080808080808080ffff04ffff02ff3affff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ffff04ff11ffff04ffff02ff36ffff04ff02ffff04ffff30ff8204f3ffff02ff3affff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ff09ff80808080ffff04ff820af3ff808080808080ff8216f380ffff04ff8203f3ff8080808080ff398080ff80808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bffff0101ff1580ffff04ff23ffff04ff53ffff04ffff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380ffff04ff820173ffff04ff5dff8080808080808080ff8080808080808080ff808080808080ffff04ffff017cffff04ff82017dffff04ffff02ff3affff04ff02ffff04ff81bdffff04ffff02ff3effff04ff02ffff04ffff04ff8205fdffff010180ff80808080ffff04ff82017dff808080808080ffff04ffff02ff3effff04ff02ffff04ffff04ff17ffff04ff8202fdff820bfd8080ff80808080ff80808080808080808080ff2f80ff8080808080ff80808080808080ffff01ff04ff17ffff04ffff04ff10ffff04ff820bfdff808080ff2f808080ff0180ffffff02ffff03ff05ffff01ff0bff81eaffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff12ffff04ff02ffff04ff0dff80808080ff808080808080ffff0181ca80ff0180ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff0bff81aaffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff12ffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff34ffff0bff34ff81caff0580ffff0bff34ff0bff818a8080ff02ffff03ff0bffff01ff30ffff02ff36ffff04ff02ffff04ff05ffff04ff1bff8080808080ff23ff3380ffff010580ff0180ffff04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff80808080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    cf1ce64fc747677745f6ceafc68588a2b20e1fbc96083f24686afba446db912b
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
    pub did_proof: LineageProof,
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
