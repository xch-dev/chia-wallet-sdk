use chia_puzzles::{
    cat::{
        CatArgs, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs, CAT_PUZZLE,
        CAT_PUZZLE_HASH, EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE,
        EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH, GENESIS_BY_COIN_ID_TAIL_PUZZLE,
        GENESIS_BY_COIN_ID_TAIL_PUZZLE_HASH,
    },
    did::{DidArgs, DID_INNER_PUZZLE, DID_INNER_PUZZLE_HASH},
    nft::{
        NftIntermediateLauncherArgs, NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NFT_INTERMEDIATE_LAUNCHER_PUZZLE, NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
        NFT_OWNERSHIP_LAYER_PUZZLE, NFT_OWNERSHIP_LAYER_PUZZLE_HASH, NFT_ROYALTY_TRANSFER_PUZZLE,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{SingletonArgs, SINGLETON_TOP_LAYER_PUZZLE, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    standard::{StandardArgs, STANDARD_PUZZLE, STANDARD_PUZZLE_HASH},
};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, TreeHash, TreeHasher};

pub trait Mod {
    const REVEAL: &[u8];
    const HASH: TreeHash;

    fn curry_tree_hash(&self) -> TreeHash
    where
        Self: Sized + ToClvm<TreeHasher>,
    {
        CurriedProgram {
            program: Self::HASH,
            args: self,
        }
        .to_clvm(&mut TreeHasher)
        .unwrap()
    }
}

impl Mod for StandardArgs {
    const REVEAL: &[u8] = &STANDARD_PUZZLE;
    const HASH: TreeHash = STANDARD_PUZZLE_HASH;
}

impl<I> Mod for CatArgs<I> {
    const REVEAL: &[u8] = &CAT_PUZZLE;
    const HASH: TreeHash = CAT_PUZZLE_HASH;
}

impl<I, M> Mod for DidArgs<I, M> {
    const REVEAL: &[u8] = &DID_INNER_PUZZLE;
    const HASH: TreeHash = DID_INNER_PUZZLE_HASH;
}

impl Mod for NftIntermediateLauncherArgs {
    const REVEAL: &[u8] = &NFT_INTERMEDIATE_LAUNCHER_PUZZLE;
    const HASH: TreeHash = NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH;
}

impl Mod for NftRoyaltyTransferPuzzleArgs {
    const REVEAL: &[u8] = &NFT_ROYALTY_TRANSFER_PUZZLE;
    const HASH: TreeHash = NFT_ROYALTY_TRANSFER_PUZZLE_HASH;
}

impl<I, P> Mod for NftOwnershipLayerArgs<I, P> {
    const REVEAL: &[u8] = &NFT_OWNERSHIP_LAYER_PUZZLE;
    const HASH: TreeHash = NFT_OWNERSHIP_LAYER_PUZZLE_HASH;
}

impl<I, M> Mod for NftStateLayerArgs<I, M> {
    const REVEAL: &[u8] = &NFT_STATE_LAYER_PUZZLE;
    const HASH: TreeHash = NFT_STATE_LAYER_PUZZLE_HASH;
}

impl<I> Mod for SingletonArgs<I> {
    const REVEAL: &[u8] = &SINGLETON_TOP_LAYER_PUZZLE;
    const HASH: TreeHash = SINGLETON_TOP_LAYER_PUZZLE_HASH;
}

impl Mod for EverythingWithSignatureTailArgs {
    const REVEAL: &[u8] = &EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE;
    const HASH: TreeHash = EVERYTHING_WITH_SIGNATURE_TAIL_PUZZLE_HASH;
}

impl Mod for GenesisByCoinIdTailArgs {
    const REVEAL: &[u8] = &GENESIS_BY_COIN_ID_TAIL_PUZZLE;
    const HASH: TreeHash = GENESIS_BY_COIN_ID_TAIL_PUZZLE_HASH;
}
