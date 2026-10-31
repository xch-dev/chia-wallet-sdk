use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT_HASH, NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH,
    SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{TreeHash, tree_hash_atom};
use hex_literal::hex;

use crate::Mod;

/// Identifies singletons with the fusion puzzle from CHIP-0021 on-chain. The launcher coin and every
/// singleton coin after the eve coin are hinted with it. This is the hash of the string `CHIP-0021`.
pub const FUSION_HINT: Bytes32 = Bytes32::new(hex!(
    "83241157e6b396cc2b428b8ad47b113de88a23a91eeed5430f086f36cc0385c9"
));

/// The inner puzzle of the fusion singleton from CHIP-0021, as compiled in the reference
/// implementation (`fusion_singleton.clsp`) and deployed on mainnet.
pub const FUSION_SINGLETON_PUZZLE: [u8; 2145] = hex!(
    "
    ff02ffff01ff02ffff03ff82bfffffff01ff02ff2affff04ff02ffff04ff09ff
    ff04ff1dffff04ff0bffff04ff17ffff04ff2fffff04ff5fffff04ff81bfffff
    04ffff02ff5effff04ff02ffff04ff8202ffffff04ff822fffff8080808080ff
    ff04ff83017fffffff04ffff02ff3affff04ff02ffff04ff09ffff04ff1dffff
    04ff17ffff04ff2fffff04ff5fffff04ff81bfffff04ffff02ff5effff04ff02
    ffff04ff82017fffff04ff825fffff8080808080ffff04ff8205ffffff04ff83
    017fffffff04ffff04ffff04ff30ffff01ff018080ffff04ffff04ff38ffff04
    ff8205ffff808080ffff04ffff04ff24ffff04ffff02ff36ffff04ff02ffff04
    ff09ffff04ffff02ff7effff04ff02ffff04ff05ff80808080ffff04ff820bff
    ff808080808080ff808080ffff04ffff04ff28ffff04ff8217ffff808080ffff
    04ffff04ff2cffff04ff820bffffff04ff8217ffffff04ffff04ff7cffff04ff
    15ff808080ff8080808080ff808080808080ff80808080808080808080808080
    ff80808080808080808080808080ffff01ff02ff2affff04ff02ffff04ff09ff
    ff04ff1dffff04ff0bffff04ff17ffff04ff2fffff04ff5fffff04ff81bfffff
    04ffff02ff5effff04ff02ffff04ff82017fffff04ff822fffff8080808080ff
    ff04ff83017fffffff04ffff02ff3affff04ff02ffff04ff09ffff04ff1dffff
    04ff17ffff04ff2fffff04ff5fffff04ff81bfffff04ffff02ff5effff04ff02
    ffff04ff8202ffffff04ff825fffff8080808080ffff04ff8205ffffff04ff83
    017fffffff04ffff04ffff04ff30ffff01ff018080ffff04ffff04ff38ffff04
    ff8205ffff808080ffff04ffff04ff24ffff04ffff02ff36ffff04ff02ffff04
    ff09ffff04ffff02ff7effff04ff02ffff04ff05ff80808080ffff04ff820bff
    ff808080808080ff808080ffff04ffff04ff28ffff04ff8217ffff808080ffff
    04ffff04ff2cffff04ff820bffffff04ff8217ffffff04ffff04ff7cffff04ff
    15ff808080ff8080808080ff808080808080ff80808080808080808080808080
    ff8080808080808080808080808080ff0180ffff04ffff01ffffffff3d52ff49
    46ffff483fff33ff3ea083241157e6b396cc2b428b8ad47b113de88a23a91eee
    d5430f086f36cc0385c9ffffff02ff02ffff03ff05ffff01ff0bff81e6ffff02
    ff2effff04ff02ffff04ff09ffff04ffff02ff32ffff04ff02ffff04ff0dff80
    808080ff808080808080ffff0181c680ff0180ffff02ffff03ff8202ffffff01
    ff04ffff04ff34ffff04ffff0bffff02ff36ffff04ff02ffff04ff05ffff04ff
    ff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff82
    08ff80ffff0bffff0101ff0b808080ffff04ffff02ff36ffff04ff02ffff04ff
    5fffff04ffff0bffff0101ff5f80ffff04ff822cffffff04ff82017fffff04ff
    ff02ff36ffff04ff02ffff04ff81bfffff04ffff0bffff0101ff81bf80ffff04
    ffff0bffff0101ff825cff80ffff04ff82bcffffff04ff2fff80808080808080
    80ff8080808080808080ff808080808080ffff02ff7effff04ff02ffff04ffff
    04ff8205ffffff04ffff04ff17ffff04ffff0101ffff04ffff04ff17ff8080ff
    80808080ff808080ff8080808080ff808080ffff02ff2affff04ff02ffff04ff
    05ffff04ff0bffff04ff17ffff04ff2fffff04ff5fffff04ff81bfffff04ff82
    017fffff04ff8206ffffff04ff8205ffffff04ff820bffff8080808080808080
    808080808080ffff01820bff80ff0180ff02ffff03ff82017fffff01ff04ffff
    04ff34ffff04ffff0bffff02ff36ffff04ff02ffff04ff05ffff04ffff0bffff
    0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff82047f80ff
    ff0bffff0101ff0b808080ffff04ffff02ff36ffff04ff02ffff04ff2fffff04
    ffff0bffff0101ff2f80ffff04ff82167fffff04ff81bfffff04ffff02ff36ff
    ff04ff02ffff04ff5fffff04ffff0bffff0101ff5f80ffff04ffff0bffff0101
    ff822e7f80ffff04ff825e7fffff04ff17ff8080808080808080ff8080808080
    808080ff808080808080ffff02ff7effff04ff02ffff04ffff04ff8205ffffff
    04ffff04ff82be7fffff04ffff0101ffff04ffff04ff82be7fff8080ff808080
    80ff808080ff8080808080ff808080ffff04ffff04ff20ffff04ffff0bff820a
    7fffff0bff8202ffff82047fff178080ff808080ffff04ffff04ff5cffff04ff
    ff0bff8202ffff82047fff1780ff808080ffff02ff3affff04ff02ffff04ff05
    ffff04ff0bffff04ff17ffff04ff2fffff04ff5fffff04ff81bfffff04ff8203
    7fffff04ff8202ffffff04ff8205ffffff04ff820bffff808080808080808080
    80808080808080ffff01820bff80ff0180ffffffffa04bf5122f344554c53bde
    2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d1
    1a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210
    fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63
    fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff0bff81
    a6ffff02ff2effff04ff02ffff04ff05ffff04ffff02ff32ffff04ff02ffff04
    ff07ff80808080ff808080808080ffff0bff22ffff0bff22ff81c6ff0580ffff
    0bff22ff0bff81868080ffff02ffff03ffff22ffff07ff0580ffff07ff0b8080
    ffff01ff04ffff04ff09ff1380ffff02ff5effff04ff02ffff04ff0dffff04ff
    1bff808080808080ffff01ff02ffff03ffff21ffff07ff0580ffff07ff0b8080
    ffff01ff0880ff8080ff018080ff0180ff02ffff03ffff07ff0580ffff01ff0b
    ffff0102ffff02ff7effff04ff02ffff04ff09ff80808080ffff02ff7effff04
    ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff0180
    80
    "
);

pub const FUSION_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    1d06c31e810fd8ba06c1559e68fc18b641386c2850eafeb71235c56c20961216
    "
));

/// The puzzle that holds the NFTs locked in a fusion singleton from CHIP-0021, as compiled in the
/// reference implementation (`p2_fusion.clsp`) and deployed on mainnet.
pub const P2_FUSION_PUZZLE: [u8; 659] = hex!(
    "
    ff02ffff01ff04ffff04ff10ffff04ff8202ffff808080ffff04ffff04ff18ff
    ff04ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff3effff04ff02ffff
    04ffff04ff05ffff04ff81bfff178080ff80808080ffff04ff82017fff808080
    808080ff808080ffff04ffff04ff14ffff04ffff0bffff02ff16ffff04ff02ff
    ff04ff05ffff04ffff02ff3effff04ff02ffff04ffff04ff05ffff04ff0bff17
    8080ff80808080ffff04ff2fff808080808080ffff0bff5fff81bfff8205ff80
    80ff808080ffff04ffff04ff2cffff04ff8205ffffff04ff8202ffffff04ffff
    04ff8205ffff8080ff8080808080ffff04ffff04ff3cffff04ffff0bff5fff81
    bfff8205ff80ff808080ff808080808080ffff04ffff01ffffff4948ff3fff33
    3cffff02ffff02ffff03ff05ffff01ff0bff81faffff02ff2effff04ff02ffff
    04ff09ffff04ffff02ff2affff04ff02ffff04ff0dff80808080ff8080808080
    80ffff0181da80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600a
    d631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b
    083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea19458
    1cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c
    1e1879b7152a6e7298a91ce119a63400ade7c5ffff0bff81baffff02ff2effff
    04ff02ffff04ff05ffff04ffff02ff2affff04ff02ffff04ff07ff80808080ff
    808080808080ffff0bff12ffff0bff12ff81daff0580ffff0bff12ff0bff819a
    8080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02
    ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ff
    ff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_FUSION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    78ba7f1159e33a5b30f7985532c33ef94ed8ef4ba117ac0372dae58829588830
    "
));

/// The fusion singleton swaps the NFTs on one side for those on the other. Only NFTs with the
/// ownership layer, the default metadata updater, and an amount of 1 can be swapped.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct FusionSingletonArgs {
    pub singleton_struct: SingletonStruct,
    pub p2_puzzle_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub metadata_updater_puzzle_hash_hash: Bytes32,
    pub nft_a_launcher_ids: Vec<Bytes32>,
    pub nft_b_launcher_ids: Vec<Bytes32>,
}

impl FusionSingletonArgs {
    pub fn new(launcher_id: Bytes32, side_a: Vec<Bytes32>, side_b: Vec<Bytes32>) -> Self {
        Self {
            singleton_struct: SingletonStruct::new(launcher_id),
            p2_puzzle_hash: P2FusionArgs::new(launcher_id).curry_tree_hash().into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            metadata_updater_puzzle_hash_hash: tree_hash_atom(&NFT_METADATA_UPDATER_DEFAULT_HASH)
                .into(),
            nft_a_launcher_ids: side_a,
            nft_b_launcher_ids: side_b,
        }
    }
}

impl Mod for FusionSingletonArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&FUSION_SINGLETON_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        FUSION_SINGLETON_PUZZLE_HASH
    }
}

/// An NFT that's being locked by a swap, which must be in the settlement puzzle. It's identified by
/// the launcher id in the same position of the curried list.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionNftToLock {
    pub coin_id: Bytes32,
    pub metadata_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub transfer_program_hash: Bytes32,
}

impl FusionNftToLock {
    pub fn new(
        coin_id: Bytes32,
        metadata_hash: Bytes32,
        current_owner: Option<Bytes32>,
        transfer_program_hash: Bytes32,
    ) -> Self {
        Self {
            coin_id,
            metadata_hash,
            current_owner,
            transfer_program_hash,
        }
    }
}

/// An NFT that's being unlocked by a swap. It's identified by the launcher id in the same position
/// of the curried list.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionNftToUnlock {
    pub coin_id: Bytes32,
    pub metadata_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub transfer_program_hash: Bytes32,
    pub target_p2_puzzle_hash: Bytes32,
}

impl FusionNftToUnlock {
    pub fn new(
        coin_id: Bytes32,
        metadata_hash: Bytes32,
        current_owner: Option<Bytes32>,
        transfer_program_hash: Bytes32,
        target_p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            coin_id,
            metadata_hash,
            current_owner,
            transfer_program_hash,
            target_p2_puzzle_hash,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionSingletonSolution {
    pub my_id: Bytes32,
    pub my_inner_puzzle_hash: Bytes32,
    pub my_amount: u64,
    pub nfts_to_lock: Vec<FusionNftToLock>,
    pub nfts_to_unlock: Vec<FusionNftToUnlock>,
    pub unlock_a: bool,
    pub offer_nonce: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2FusionArgs {
    pub singleton_mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
}

impl P2FusionArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
    }
}

impl Mod for P2FusionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_FUSION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_FUSION_PUZZLE_HASH
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2FusionSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub singleton_coin_id: Bytes32,
    pub nft_launcher_id: Bytes32,
    pub nft_inner_puzzle_hash: Bytes32,
    pub my_amount: u64,
    pub next_puzzle_hash: Bytes32,
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::singleton::SingletonArgs;
    use chia_sha2::Sha256;

    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_fusion_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(FUSION_SINGLETON_PUZZLE => FUSION_SINGLETON_PUZZLE_HASH);
        assert_puzzle_hash!(P2_FUSION_PUZZLE => P2_FUSION_PUZZLE_HASH);
        Ok(())
    }

    #[test]
    fn test_fusion_hint() {
        let mut hasher = Sha256::new();
        hasher.update(b"CHIP-0021");
        assert_eq!(hasher.finalize(), FUSION_HINT.to_bytes());
    }

    /// Puzzle hashes of a fusion singleton and a locked NFT's p2 puzzle deployed on mainnet.
    #[test]
    fn test_mainnet_fusion_puzzle_hashes() {
        let launcher_id = Bytes32::new(hex!(
            "29e6c03fe0fbcca3d4edf2c7e703c4d0fb2a3cdfe157f57c0dfd3a67abeb4d36"
        ));
        let nft_a = Bytes32::new(hex!(
            "6f8603f7521fda4f62148b96ec2a0ef63ddb0943f36ef08c4ae598339c1c2353"
        ));
        let nft_b = Bytes32::new(hex!(
            "fcd4b808ad2e3d6c07b512552b7a8facd12b69cfb64979b5c776e91aec89fea3"
        ));

        let inner_puzzle_hash =
            FusionSingletonArgs::new(launcher_id, vec![nft_a], vec![nft_b]).curry_tree_hash();

        assert_eq!(
            hex::encode(inner_puzzle_hash),
            "8fb51fa546975dd05fe4eb88a6423b73054207751dbfbe3c742c6652860077b3"
        );
        assert_eq!(
            hex::encode(SingletonArgs::curry_tree_hash(
                launcher_id,
                inner_puzzle_hash
            )),
            "c4b70a7b6f397952f7a575bfcea6f3cc2e890900965e2de34c8db803d31636ea"
        );
    }
}
