use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{MerkleProof, Mod};

/// Serialized reward distributor lock puzzle for staking curated NFTs, compiled from Rue.
///
/// The curated list lives in a `DataLayer` store whose Merkle root has a leaf for each
/// eligible NFT: the tree hash of `(launcher_id . shares)`. The puzzle proves the
/// store's current root by asserting the store's oracle puzzle announcement (`"$"`).
/// For each NFT, it checks an inclusion proof against that root, verifies that the NFT is
/// paid to the distributor's p2 puzzle hash through a settlement payment, and creates a
/// deposit slot. Each NFT is worth the number of shares in its leaf.
pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE: [u8; 1233] = hex!(
    // Rue
    "
    ff02ffff01ff02ff16ffff04ffff04ff04ffff04ff0affff04ff2effff04ff16
    ff3e80808080ffff04ff03ffff04ff820bffffff01ff808080808080ffff04ff
    ff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff
    04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff
    038080ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bff
    ff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff
    02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04ffff01ff02ff
    ff03ff0bffff01ff02ffff01ff02ffff03ffff09ff825ffbffff02ff7dffff04
    ff7dffff04ff820fe7ffff0bffff0101ffff02ff09ffff04ff09ffff04ff47ff
    820be78080808080808080ffff01ff02ffff03ffff15ff820be7ff8080ffff01
    ff02ff5dffff04ff05ffff04ff0bffff04ff37ffff04ffff10ff2fff820be780
    ffff04ffff04ffff0133ffff04ffff02ff15ffff04ff2dffff04ff82017bffff
    04ffff0bffff0101ffff02ff09ffff04ff09ffff04ff820bfbffff04ff820be7
    ff478080808080ff8080808080ffff04ff80ffff04ffff04ff820bfbff8080ff
    8080808080ffff04ffff04ffff013fffff04ff02ff808080ffff04ffff04ffff
    013effff04ffff0effff016cff0280ff808080ff3f8080808080808080ffff01
    ff088080ff0180ffff01ff088080ff0180ffff04ffff0bffff02ff0affff04ff
    16ffff04ff11ffff04ffff02ff04ffff04ff04ffff04ff11ffff04ff23ff3980
    808080ffff04ffff02ff0affff04ff16ffff04ff15ffff04ffff0bffff0101ff
    1580ffff04ff53ffff04ff81b3ffff04ffff02ff0affff04ff16ffff04ff2dff
    ff04ffff0bffff0101ff2d80ffff04ffff0bffff0101ff82017380ffff04ff82
    02f3ffff04ff5dff8080808080808080ff8080808080808080ff808080808080
    ffff02ff04ffff04ff04ffff04ffff02ff04ffff04ff04ffff04ff17ffff04ff
    8202fdff820bfd80808080ffff04ffff04ff82017dffff04ffff0101ffff04ff
    ff04ffff02ff04ffff04ff04ffff04ff8205fdff82017d808080ff8080ff8080
    8080ff808080808080ff018080ffff01ff04ff17ffff04ffff04ffff013fffff
    04ffff0bffff02ff0affff04ff16ffff04ff11ffff04ffff02ff04ffff04ff04
    ff098080ffff04ffff02ff0affff04ff16ffff04ff15ffff04ffff0bffff0101
    ff1580ffff04ffff0bffff0102ffff0bffff0101ff822ffd80ffff02ffff03ff
    825ffdffff01825ffdffff01ff0bffff01018080ff018080ffff04ff82bffdff
    ff04ff82fffdff8080808080808080ff808080808080ffff012480ff808080ff
    ff04ffff04ffff0146ffff04ff820bfdff808080ff1f80808080ff0180ffff04
    ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0b
    ffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ff
    ff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff01
    8201018080ff0180ffff01ff02ffff03ff0dffff01ff02ff02ffff04ff02ffff
    04ffff04ffff17ff09ffff0181ff80ff1d80ffff0bffff0102ffff03ffff18ff
    09ffff010180ff15ff0780ffff03ffff18ff09ffff010180ff07ff1580808080
    80ffff010780ff018080808080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE`].
pub const REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b53e7d84cdf203fbbb2a56982d68af7f559976bb9b869b56b1e3e17a9008fb1a
    "
));

/// Curried arguments of [`REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorNftsFromDlLockingPuzzleArgs {
    /// Singleton struct of the `DataLayer` store that holds the curated list.
    pub dl_singleton_struct: SingletonStruct,
    /// Mod hash of the NFT state layer.
    pub nft_state_layer_mod_hash: Bytes32,
    /// Mod hash of the NFT ownership layer.
    pub nft_ownership_layer_mod_hash: Bytes32,
    /// Mod hash of the settlement payments puzzle.
    pub offer_mod_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for deposit slots
    /// ([`RewardDistributorSlotNonce::DEPOSIT`](crate::puzzles::RewardDistributorSlotNonce::DEPOSIT)).
    pub deposit_slot_1st_curry_hash: Bytes32,
    /// Puzzle hash that holds staked assets: [`P2DelegatedBySingletonLayerArgs`](crate::puzzles::P2DelegatedBySingletonLayerArgs)
    /// for the reward distributor singleton, with nonce 1.
    pub my_p2_puzzle_hash: Bytes32,
}

impl RewardDistributorNftsFromDlLockingPuzzleArgs {
    /// Creates the arguments with the standard NFT and settlement payments mod hashes.
    pub fn new(
        store_launcher_id: Bytes32,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
    ) -> Self {
        Self {
            dl_singleton_struct: SingletonStruct::new(store_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            deposit_slot_1st_curry_hash,
            my_p2_puzzle_hash,
        }
    }
}

/// Information about an NFT staked with [`REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE`].
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StakeNftFromDlInfo {
    /// Launcher id of the NFT.
    pub nft_launcher_id: Bytes32,
    /// Tree hash of the NFT's metadata.
    pub nft_metadata_hash: Bytes32,
    /// Tree hash of the NFT's metadata updater puzzle hash.
    pub nft_metadata_updater_hash_hash: Bytes32,
    /// Current owner (DID launcher id) of the NFT, if any.
    pub nft_owner: Option<Bytes32>,
    /// Puzzle hash of the NFT's royalty transfer program.
    pub nft_transfer_porgram_hash: Bytes32,
    /// Shares the NFT is worth, as listed in the store.
    pub nft_shares: u64,
    /// Proof that `(nft_launcher_id . nft_shares)` is in the store's Merkle root.
    #[clvm(rest)]
    pub nft_inclusion_proof: MerkleProof,
}

/// Solution to [`REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE`], after the ephemeral
/// state and custody puzzle hash.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorNftsFromDlLockingPuzzleSolution {
    /// Coin id of the reward distributor singleton coin being spent.
    pub my_id: Bytes32,
    /// NFTs being staked.
    pub nft_infos: Vec<StakeNftFromDlInfo>,
    /// Current Merkle root of the `DataLayer` store.
    pub dl_root_hash: Bytes32,
    /// Tree hash of the store's metadata after the root hash.
    pub dl_metadata_rest_hash: Option<Bytes32>,
    /// Tree hash of the store's metadata updater puzzle hash.
    pub dl_metadata_updater_hash_hash: Bytes32,
    /// Puzzle hash of the store's inner puzzle below the NFT state layer (its delegation
    /// layer).
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
