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

/// The tree hash of [`FUSION_SINGLETON_PUZZLE`].
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

/// The tree hash of [`P2_FUSION_PUZZLE`].
pub const P2_FUSION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    78ba7f1159e33a5b30f7985532c33ef94ed8ef4ba117ac0372dae58829588830
    "
));

/// The fusion singleton swaps the NFTs on one side for those on the other. Only NFTs with the
/// ownership layer, the default metadata updater, and an amount of 1 can be swapped.
///
/// These are the curried arguments of [`FUSION_SINGLETON_PUZZLE`], which is the inner puzzle of a
/// singleton. Each spend unlocks every NFT on one side, which are held by the [`P2FusionArgs`]
/// puzzle, and locks every NFT on the other side by sending it to that puzzle. NFTs move through
/// the settlement payments puzzle in both directions, so a swap can be the other side of an offer.
/// The singleton recreates itself unchanged, hinted with [`FUSION_HINT`], and can't be spent in
/// the same block that it was created in.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_puzzle_types::singleton::SingletonArgs;
/// # use chia_sdk_types::{Mod, puzzles::{FusionSingletonArgs, P2FusionArgs}};
/// let launcher_id = Bytes32::new([1; 32]);
/// let fused_nft = Bytes32::new([2; 32]);
/// let components = vec![Bytes32::new([3; 32]), Bytes32::new([4; 32])];
///
/// let args = FusionSingletonArgs::new(launcher_id, vec![fused_nft], components);
///
/// // Locked NFTs are held by the p2 fusion puzzle of the same singleton.
/// assert_eq!(
///     args.p2_puzzle_hash,
///     Bytes32::from(P2FusionArgs::new(launcher_id).curry_tree_hash())
/// );
///
/// // The fusion puzzle is the inner puzzle of the singleton.
/// let inner_puzzle_hash = args.curry_tree_hash();
/// let puzzle_hash = SingletonArgs::curry_tree_hash(launcher_id, inner_puzzle_hash);
/// # let _ = puzzle_hash;
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct FusionSingletonArgs {
    /// The singleton struct of the fusion singleton itself.
    pub singleton_struct: SingletonStruct,
    /// The puzzle hash of the [`P2FusionArgs`] puzzle for this singleton, which locked NFTs are
    /// sent to as their p2 puzzle hash.
    pub p2_puzzle_hash: Bytes32,
    /// The mod hash of the settlement payments puzzle, which NFTs pass through when they're locked
    /// or unlocked.
    pub offer_mod_hash: Bytes32,
    /// The mod hash of the NFT state layer.
    pub nft_state_layer_mod_hash: Bytes32,
    /// The mod hash of the NFT ownership layer.
    pub nft_ownership_layer_mod_hash: Bytes32,
    /// The tree hash of the metadata updater puzzle hash as an atom. Only NFTs with this metadata
    /// updater can be swapped.
    pub metadata_updater_puzzle_hash_hash: Bytes32,
    /// The launcher ids of the NFTs on side A, in the order that the solution lists them.
    pub nft_a_launcher_ids: Vec<Bytes32>,
    /// The launcher ids of the NFTs on side B, in the order that the solution lists them.
    pub nft_b_launcher_ids: Vec<Bytes32>,
}

impl FusionSingletonArgs {
    /// Creates the arguments for the fusion singleton with the given launcher id, which swaps the
    /// NFTs with the launcher ids in `side_a` for those in `side_b`.
    ///
    /// The other arguments are the standard values used on mainnet: the singleton and NFT layer
    /// mod hashes, the settlement payments mod hash, the default metadata updater, and the
    /// [`P2FusionArgs`] puzzle hash for the launcher id.
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
///
/// The fusion singleton asserts that the NFT pays itself to the [`P2FusionArgs`] puzzle hash
/// with the swap's offer nonce. The NFT's full puzzle hash is derived from these fields, so they
/// must match the NFT exactly.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionNftToLock {
    /// The coin id of the NFT. The puzzle doesn't check it when locking.
    pub coin_id: Bytes32,
    /// The tree hash of the NFT's metadata.
    pub metadata_hash: Bytes32,
    /// The current owner (usually a DID launcher id) in the NFT's ownership layer, if any.
    pub current_owner: Option<Bytes32>,
    /// The tree hash of the NFT's transfer program.
    pub transfer_program_hash: Bytes32,
}

impl FusionNftToLock {
    /// Creates a new [`FusionNftToLock`] from the NFT's coin id and the values in its layers.
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
///
/// The locked NFT is spent with the [`P2FusionArgs`] puzzle, which sends it to the settlement
/// payments puzzle. The fusion singleton then asserts that it pays itself from there to
/// `target_p2_puzzle_hash` with the swap's offer nonce.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionNftToUnlock {
    /// The coin id of the locked NFT, which is held by the [`P2FusionArgs`] puzzle.
    pub coin_id: Bytes32,
    /// The tree hash of the NFT's metadata.
    pub metadata_hash: Bytes32,
    /// The current owner (usually a DID launcher id) in the NFT's ownership layer, if any.
    pub current_owner: Option<Bytes32>,
    /// The tree hash of the NFT's transfer program.
    pub transfer_program_hash: Bytes32,
    /// The p2 puzzle hash that the NFT is sent to once it's unlocked.
    pub target_p2_puzzle_hash: Bytes32,
}

impl FusionNftToUnlock {
    /// Creates a new [`FusionNftToUnlock`] from the locked NFT's coin id, the values in its
    /// layers, and the p2 puzzle hash to send it to.
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

/// The solution of the fusion singleton. It isn't signed, and the puzzle ignores or loosely reads
/// parts of it, so a valid spend on-chain may not parse as this type. To get the direction of a
/// confirmed spend, use `Fusion::parse_child` in `chia-sdk-driver`.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct FusionSingletonSolution {
    /// The coin id of the fusion singleton coin being spent.
    pub my_id: Bytes32,
    /// The inner puzzle hash of the fusion singleton, which it's recreated with.
    pub my_inner_puzzle_hash: Bytes32,
    /// The amount of the fusion singleton coin.
    pub my_amount: u64,
    /// The NFTs on the side being locked, in the same order as their launcher ids are curried.
    /// The length must match the number of launcher ids on that side.
    pub nfts_to_lock: Vec<FusionNftToLock>,
    /// The NFTs on the side being unlocked, in the same order as their launcher ids are curried.
    /// The length must match the number of launcher ids on that side.
    pub nfts_to_unlock: Vec<FusionNftToUnlock>,
    /// If `true`, the NFTs on side A are unlocked and those on side B are locked. Otherwise, side
    /// B is unlocked and side A is locked.
    pub unlock_a: bool,
    /// The nonce of the settlement payments that every NFT in the swap makes.
    pub offer_nonce: Bytes32,
}

/// Curried arguments of [`P2_FUSION_PUZZLE`], which holds NFTs that are locked in a fusion
/// singleton. It's used as the NFT's p2 puzzle, inside the ownership layer.
///
/// The NFT can only be spent alongside the fusion singleton with `launcher_id`, which must
/// announce the NFT's launcher id and the puzzle hash to send it to. The fusion singleton always
/// sends unlocked NFTs to the settlement payments puzzle. The p2 puzzle doesn't output a transfer
/// condition, so an NFT keeps its current owner when it's locked or unlocked.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2FusionArgs {
    /// The mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// The launcher id of the fusion singleton that the NFT is locked in.
    pub launcher_id: Bytes32,
    /// The puzzle hash of the singleton launcher.
    pub launcher_puzzle_hash: Bytes32,
}

impl P2FusionArgs {
    /// Creates the arguments for the fusion singleton with the given launcher id, with the
    /// standard singleton mod and launcher puzzle hashes.
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

/// The solution of the [`P2FusionArgs`] puzzle, which unlocks an NFT during a swap.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2FusionSolution {
    /// The inner puzzle hash of the fusion singleton that's being spent.
    pub singleton_inner_puzzle_hash: Bytes32,
    /// The coin id of the fusion singleton that's being spent, which must announce this unlock.
    pub singleton_coin_id: Bytes32,
    /// The launcher id of the NFT being unlocked.
    pub nft_launcher_id: Bytes32,
    /// The singleton inner puzzle hash of the NFT (its state layer), which is used to check the
    /// NFT's full puzzle hash.
    pub nft_inner_puzzle_hash: Bytes32,
    /// The amount of the NFT coin.
    pub my_amount: u64,
    /// The p2 puzzle hash to send the NFT to, which must be the one the fusion singleton announced.
    /// For the fusion singleton, this is always the settlement payments mod hash.
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
