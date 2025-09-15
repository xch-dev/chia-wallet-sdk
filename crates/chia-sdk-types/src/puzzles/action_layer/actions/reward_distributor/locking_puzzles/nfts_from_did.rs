use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE: [u8; 1054] = hex!(
    "
    ff02ffff01ff02ff3cffff04ff02ffff04ff03ffff04ff8207ffffff01ff80ff
    808080808080ffff04ffff01ffffff46ff3f3eff02ffff04ffff04ff28ffff04
    ff05ff808080ffff04ffff04ff38ffff04ff05ff808080ff0b8080ff02ffff03
    ff0bffff01ff02ff3cffff04ff02ffff04ff05ffff04ff1bffff04ffff10ff17
    ffff010180ffff04ffff02ff2cffff04ff02ffff04ffff0bffff02ff3affff04
    ff02ffff04ff11ffff04ffff02ff3effff04ff02ffff04ffff04ff11ffff04ff
    ff02ff36ffff04ff02ffff04ffff30ff8204f3ffff02ff3affff04ff02ffff04
    ff11ffff04ffff02ff3effff04ff02ffff04ff09ff80808080ffff04ff820af3
    ff808080808080ff8216f380ffff04ff8203f3ff8080808080ff398080ff8080
    8080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bffff0101ff15
    80ffff04ff23ffff04ff53ffff04ffff02ff3affff04ff02ffff04ff2dffff04
    ffff0bffff0101ff2d80ffff04ffff0bffff0101ff81b380ffff04ff820173ff
    ff04ff5dff8080808080808080ff8080808080808080ff808080808080ffff02
    ff3effff04ff02ffff04ffff04ffff02ff3effff04ff02ffff04ffff04ff8202
    fdff820bfd80ff80808080ffff04ffff02ff2effff04ff02ffff04ffff02ff3a
    ffff04ff02ffff04ff81bdffff04ffff02ff3effff04ff02ffff04ffff04ff82
    05fdffff010180ff80808080ffff04ff82017dff808080808080ff80808080ff
    808080ff8080808080ffff04ff2fff8080808080ff80808080808080ffff01ff
    04ff17ffff04ffff04ff10ffff04ff820bfdff808080ff2f808080ff0180ffff
    ff02ffff03ff05ffff01ff0bff81eaffff02ff26ffff04ff02ffff04ff09ffff
    04ffff02ff12ffff04ff02ffff04ff0dff80808080ff808080808080ffff0181
    ca80ff0180ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385
    a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e8
    78a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531
    e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7
    152a6e7298a91ce119a63400ade7c5ff0bff81aaffff02ff26ffff04ff02ffff
    04ff05ffff04ffff02ff12ffff04ff02ffff04ff07ff80808080ff8080808080
    80ffffff0bff14ffff0bff14ff81caff0580ffff0bff14ff0bff818a8080ff02
    ffff03ff0bffff01ff30ffff02ff36ffff04ff02ffff04ff05ffff04ff1bff80
    80808080ff23ff3380ffff010580ff0180ffff04ff05ffff04ffff0101ffff04
    ffff04ff05ff8080ff80808080ff02ffff03ffff07ff0580ffff01ff0bffff01
    02ffff02ff3effff04ff02ffff04ff09ff80808080ffff02ff3effff04ff02ff
    ff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    e172b5d1dde63f64b5f7dc181e54a5b49fed0987f83ddc9dfe2bfbefdba5b5bc
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
