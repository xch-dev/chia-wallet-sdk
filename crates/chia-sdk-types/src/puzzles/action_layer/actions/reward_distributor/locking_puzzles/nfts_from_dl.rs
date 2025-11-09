use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE: [u8; 1285] = hex!(
    "
    ff02ffff01ff02ff3cffff04ff02ffff04ff03ffff04ff820bffffff01ff80ff
    808080808080ffff04ffff01ffffff46ff3f3eff02ffff04ffff04ff28ffff04
    ff05ff808080ffff04ffff04ff38ffff04ff05ff808080ff0b8080ff02ffff03
    ff0bffff01ff02ff3cffff04ff02ffff04ff05ffff04ff1bffff04ffff10ff17
    ff8205f380ffff04ffff02ff2cffff04ff02ffff04ffff0bffff02ff3affff04
    ff02ffff04ff11ffff04ffff02ff2effff04ff02ffff04ffff04ff11ffff04ff
    23ff398080ff80808080ffff04ffff02ff3affff04ff02ffff04ff15ffff04ff
    ff0bffff0101ff1580ffff04ff53ffff04ff81b3ffff04ffff02ff3affff04ff
    02ffff04ff2dffff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff8201
    7380ffff04ff8202f3ffff04ff5dff8080808080808080ff8080808080808080
    ff808080808080ffff02ff2effff04ff02ffff04ffff04ffff02ff2effff04ff
    02ffff04ffff04ff17ffff04ff8202fdff820bfd8080ff80808080ffff04ffff
    02ff36ffff04ff02ffff04ffff02ff3affff04ff02ffff04ff81bdffff04ffff
    02ff2effff04ff02ffff04ffff04ff8205fdff8205f380ff80808080ffff04ff
    82017dff808080808080ff80808080ff808080ff8080808080ffff04ffff02ff
    ff03ffff09ff822ffdffff02ff3effff04ff02ffff04ffff0bffff0101ffff02
    ff2effff04ff02ffff04ffff04ff8205fdff8205f380ff8080808080ffff04ff
    8207f3ff808080808080ffff012fffff01ff088080ff0180ff8080808080ff80
    808080808080ffff01ff04ff17ffff04ffff04ff28ffff04ffff0bffff02ff3a
    ffff04ff02ffff04ff11ffff04ffff02ff2effff04ff02ffff04ff09ff808080
    80ffff04ffff02ff3affff04ff02ffff04ff15ffff04ffff0bffff0101ff1580
    ffff04ffff02ff2effff04ff02ffff04ffff03ff825ffdffff04ff822ffdff82
    5ffd80ff822ffd80ff80808080ffff04ff82bffdffff04ff82fffdff80808080
    80808080ff808080808080ffff012480ff808080ffff04ffff04ff10ffff04ff
    820bfdff808080ff2f80808080ff0180ffffff02ffff03ff05ffff01ff0bff81
    eaffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff12ffff04ff02ffff04
    ff0dff80808080ff808080808080ffff0181ca80ff0180ffffffa04bf5122f34
    4554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a1
    84f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a1
    2871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a1
    02a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7
    c5ff0bff81aaffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff12ffff04
    ff02ffff04ff07ff80808080ff808080808080ffffff0bff14ffff0bff14ff81
    caff0580ffff0bff14ff0bff818a8080ff04ff05ffff04ffff0101ffff04ffff
    04ff05ff8080ff80808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff02ffff03ff1b
    ffff01ff02ff3effff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380ff
    ff01ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180ff
    ff04ffff04ffff17ff13ffff0181ff80ff3b80ff8080808080ffff010580ff01
    80ff018080
    "
);

pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    6c32f047c518d98bdaa0865d39f6f1d6d5a81a3e8bec8b0f50d7ff11d8b7c923
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
pub struct RewardDistributorNftsFromDlLockingPuzzleSolution<MR> {
    pub my_id: Bytes32,
    pub nft_infos: Vec<StakeNftFromDlInfo>,
    pub dl_root_hash: Bytes32,
    pub dl_metadata_rest: MR,
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
