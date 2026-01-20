use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{puzzles::NONCE_WRAPPER_PUZZLE_HASH, MerkleProof, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE: [u8; 1294] = hex!(
    "
    ff02ffff01ff02ff3cffff04ff02ffff04ff03ffff04ff820bffffff01ff80ff
    808080808080ffff04ffff01ffffff46ff3f3eff02ffff04ffff04ff28ffff04
    ff05ff808080ffff04ffff04ff38ffff04ffff0effff016cff0580ff808080ff
    0b8080ff02ffff03ff0bffff01ff02ff3cffff04ff02ffff04ff05ffff04ff1b
    ffff04ffff10ff17ff8205f380ffff04ffff02ff2cffff04ff02ffff04ffff0b
    ffff02ff3affff04ff02ffff04ff11ffff04ffff02ff2effff04ff02ffff04ff
    ff04ff11ffff04ff23ff398080ff80808080ffff04ffff02ff3affff04ff02ff
    ff04ff15ffff04ffff0bffff0101ff1580ffff04ff53ffff04ff81b3ffff04ff
    ff02ff3affff04ff02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff
    0bffff0101ff82017380ffff04ff8202f3ffff04ff5dff8080808080808080ff
    8080808080808080ff808080808080ffff02ff2effff04ff02ffff04ffff04ff
    ff02ff2effff04ff02ffff04ffff04ff17ffff04ff8202fdff820bfd8080ff80
    808080ffff04ffff02ff36ffff04ff02ffff04ffff02ff3affff04ff02ffff04
    ff81bdffff04ffff02ff2effff04ff02ffff04ffff04ff8205fdff8205f380ff
    80808080ffff04ff82017dff808080808080ff80808080ff808080ff80808080
    80ffff04ffff02ffff03ffff09ff822ffdffff02ff3effff04ff02ffff04ffff
    0bffff0101ffff02ff2effff04ff02ffff04ffff04ff23ff8205f380ff808080
    8080ffff04ff8207f3ff808080808080ffff012fffff01ff088080ff0180ff80
    80808080ff80808080808080ffff01ff04ff17ffff04ffff04ff28ffff04ffff
    0bffff02ff3affff04ff02ffff04ff11ffff04ffff02ff2effff04ff02ffff04
    ff09ff80808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bff
    ff0101ff1580ffff04ffff0bffff0102ffff0bffff0101ff822ffd80ffff02ff
    ff03ff825ffdffff01825ffdffff01818a80ff018080ffff04ff82bffdffff04
    ff82fffdff8080808080808080ff808080808080ffff012480ff808080ffff04
    ffff04ff10ffff04ff820bfdff808080ff2f80808080ff0180ffffff02ffff03
    ff05ffff01ff0bff81eaffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff
    12ffff04ff02ffff04ff0dff80808080ff808080808080ffff0181ca80ff0180
    ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c
    7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f5
    96718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d2
    25f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298
    a91ce119a63400ade7c5ff0bff81aaffff02ff26ffff04ff02ffff04ff05ffff
    04ffff02ff12ffff04ff02ffff04ff07ff80808080ff808080808080ffffff0b
    ff14ffff0bff14ff81caff0580ffff0bff14ff0bff818a8080ff04ff05ffff04
    ffff0101ffff04ffff04ff05ff8080ff80808080ffff02ffff03ffff07ff0580
    ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02
    ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff
    0180ff02ffff03ff1bffff01ff02ff3effff04ff02ffff04ffff02ffff03ffff
    18ffff0101ff1380ffff01ff0bffff0102ff2bff0580ffff01ff0bffff0102ff
    05ff2b8080ff0180ffff04ffff04ffff17ff13ffff0181ff80ff3b80ff808080
    8080ffff010580ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    f7c308010f1cc99e32d7a6faf9264656cb4d5663e487dc5127253e9dfaa54543
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

#[derive(FromClvm, ToClvm, Debug, PartialEq, Eq)]
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
