use chia_puzzle_types::{
    cat::{CatArgs, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs},
    did::DidArgs,
    nft::{
        NftIntermediateLauncherArgs, NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs,
    },
    singleton::SingletonArgs,
    standard::StandardArgs,
};
use chia_puzzles::{
    CAT_PUZZLE, CAT_PUZZLE_HASH, DID_INNERPUZ, DID_INNERPUZ_HASH, EVERYTHING_WITH_SIGNATURE,
    EVERYTHING_WITH_SIGNATURE_HASH, GENESIS_BY_COIN_ID, GENESIS_BY_COIN_ID_HASH,
    NFT_INTERMEDIATE_LAUNCHER, NFT_INTERMEDIATE_LAUNCHER_HASH, NFT_OWNERSHIP_LAYER,
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES,
    NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH, NFT_STATE_LAYER,
    NFT_STATE_LAYER_HASH, P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
    P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH, SINGLETON_TOP_LAYER_V1_1,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, TreeHash, TreeHasher};

/// This trait makes it possible to get the mod hash or puzzle reveal of a puzzle.
///
/// There is also a utility for calculating the curried tree hash, provided the type
/// implements [`ToTreeHash`](clvm_utils::ToTreeHash). This is much more efficient than
/// manually allocating and hashing the puzzle and its arguments.
///
/// This trait should be be implemented for types that represent the curried arguments of puzzles.
/// However, if a puzzle can't be curried (ie it has no arguments), this trait  can still be
/// implemented on a marker struct that doesn't implement [`ToTreeHash`](clvm_utils::ToTreeHash).
/// This will disable the [`curry_tree_hash`](Mod::curry_tree_hash) method.
///
/// ## Usage Example
///
/// We can specify the arguments of a puzzle to get its curried puzzle hash.
///
/// ```rust
/// # use chia_bls::PublicKey;
/// # use chia_puzzle_types::standard::StandardArgs;
/// # use chia_sdk_types::Mod;
/// let args = StandardArgs::new(PublicKey::default());
/// let puzzle_hash = args.curry_tree_hash();
/// ```
pub trait Mod {
    const MOD_REVEAL: &[u8];
    const MOD_HASH: TreeHash;

    /// Curry the arguments into the [`MOD_HASH`](Mod::MOD_HASH).
    fn curry_tree_hash(&self) -> TreeHash
    where
        Self: Sized + ToClvm<TreeHasher>,
    {
        CurriedProgram {
            program: Self::MOD_HASH,
            args: self,
        }
        .to_clvm(&mut TreeHasher)
        .unwrap()
    }
}

impl<T> Mod for &T
where
    T: Mod,
{
    const MOD_REVEAL: &'static [u8] = T::MOD_REVEAL;
    const MOD_HASH: TreeHash = T::MOD_HASH;
}

impl Mod for StandardArgs {
    const MOD_REVEAL: &[u8] = &P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE;
    const MOD_HASH: TreeHash = TreeHash::new(P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH);
}

impl<I> Mod for CatArgs<I> {
    const MOD_REVEAL: &[u8] = &CAT_PUZZLE;
    const MOD_HASH: TreeHash = TreeHash::new(CAT_PUZZLE_HASH);
}

impl<I, M> Mod for DidArgs<I, M> {
    const MOD_REVEAL: &[u8] = &DID_INNERPUZ;
    const MOD_HASH: TreeHash = TreeHash::new(DID_INNERPUZ_HASH);
}

impl Mod for NftIntermediateLauncherArgs {
    const MOD_REVEAL: &[u8] = &NFT_INTERMEDIATE_LAUNCHER;
    const MOD_HASH: TreeHash = TreeHash::new(NFT_INTERMEDIATE_LAUNCHER_HASH);
}

impl Mod for NftRoyaltyTransferPuzzleArgs {
    const MOD_REVEAL: &[u8] = &NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES;
    const MOD_HASH: TreeHash =
        TreeHash::new(NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH);
}

impl<I, P> Mod for NftOwnershipLayerArgs<I, P> {
    const MOD_REVEAL: &[u8] = &NFT_OWNERSHIP_LAYER;
    const MOD_HASH: TreeHash = TreeHash::new(NFT_OWNERSHIP_LAYER_HASH);
}

impl<I, M> Mod for NftStateLayerArgs<I, M> {
    const MOD_REVEAL: &[u8] = &NFT_STATE_LAYER;
    const MOD_HASH: TreeHash = TreeHash::new(NFT_STATE_LAYER_HASH);
}

impl<I> Mod for SingletonArgs<I> {
    const MOD_REVEAL: &[u8] = &SINGLETON_TOP_LAYER_V1_1;
    const MOD_HASH: TreeHash = TreeHash::new(SINGLETON_TOP_LAYER_V1_1_HASH);
}

impl Mod for EverythingWithSignatureTailArgs {
    const MOD_REVEAL: &[u8] = &EVERYTHING_WITH_SIGNATURE;
    const MOD_HASH: TreeHash = TreeHash::new(EVERYTHING_WITH_SIGNATURE_HASH);
}

impl Mod for GenesisByCoinIdTailArgs {
    const MOD_REVEAL: &[u8] = &GENESIS_BY_COIN_ID;
    const MOD_HASH: TreeHash = TreeHash::new(GENESIS_BY_COIN_ID_HASH);
}
