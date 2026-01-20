use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE: [u8; 1068] = hex!(
    "
    ff02ffff01ff02ff3cffff04ff02ffff04ff03ffff04ff8207ffffff01ff80ff
    808080808080ffff04ffff01ffffff46ff3f3eff02ffff04ffff04ff28ffff04
    ff05ff808080ffff04ffff04ff38ffff04ffff0effff016cff0580ff808080ff
    0b8080ff02ffff03ff0bffff01ff02ff3cffff04ff02ffff04ff05ffff04ff1b
    ffff04ffff10ff17ffff010180ffff04ffff02ff2cffff04ff02ffff04ffff0b
    ffff02ff3affff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ff
    ff04ff11ffff04ffff02ff36ffff04ff02ffff04ffff30ff8204f3ffff02ff3a
    ffff04ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ff09ff808080
    80ffff04ff820af3ff808080808080ff8216f380ffff04ff8203f3ff80808080
    80ff398080ff80808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ff
    ff0bffff0101ff1580ffff04ff23ffff04ff53ffff04ffff02ff3affff04ff02
    ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380
    ffff04ff820173ffff04ff5dff8080808080808080ff8080808080808080ff80
    8080808080ffff02ff3effff04ff02ffff04ffff04ffff02ff3effff04ff02ff
    ff04ffff04ff17ffff04ff8202fdff820bfd8080ff80808080ffff04ffff02ff
    2effff04ff02ffff04ffff02ff3affff04ff02ffff04ff81bdffff04ffff02ff
    3effff04ff02ffff04ffff04ff8205fdffff010180ff80808080ffff04ff8201
    7dff808080808080ff80808080ff808080ff8080808080ffff04ff2fff808080
    8080ff80808080808080ffff01ff04ff17ffff04ffff04ff10ffff04ff820bfd
    ff808080ff2f808080ff0180ffffff02ffff03ff05ffff01ff0bff81eaffff02
    ff26ffff04ff02ffff04ff09ffff04ffff02ff12ffff04ff02ffff04ff0dff80
    808080ff808080808080ffff0181ca80ff0180ffffffa04bf5122f344554c53b
    de2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623
    d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee2
    10fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd
    63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff0bff
    81aaffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff12ffff04ff02ffff
    04ff07ff80808080ff808080808080ffffff0bff14ffff0bff14ff81caff0580
    ffff0bff14ff0bff818a8080ff02ffff03ff0bffff01ff30ffff02ff36ffff04
    ff02ffff04ff05ffff04ff1bff8080808080ff23ff3380ffff010580ff0180ff
    ff04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff80808080ff02ffff03
    ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80
    808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff01
    01ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    41d1a1591392fd801f64bbbcbf8373843c75f1b2f6926e5e4bb5da94c5d9c8a2
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
