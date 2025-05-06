use std::borrow::Cow;

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
    fn mod_reveal() -> Cow<'static, [u8]>;
    fn mod_hash() -> TreeHash;

    /// Curry the arguments into the [`MOD_HASH`](Mod::MOD_HASH).
    fn curry_tree_hash(&self) -> TreeHash
    where
        Self: Sized + ToClvm<TreeHasher>,
    {
        CurriedProgram {
            program: Self::mod_hash(),
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
    fn mod_reveal() -> Cow<'static, [u8]> {
        T::mod_reveal()
    }

    fn mod_hash() -> TreeHash {
        T::mod_hash()
    }
}

impl Mod for StandardArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH)
    }
}

impl<I> Mod for CatArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&CAT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(CAT_PUZZLE_HASH)
    }
}

impl<I, M> Mod for DidArgs<I, M> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DID_INNERPUZ)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(DID_INNERPUZ_HASH)
    }
}

impl Mod for NftIntermediateLauncherArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NFT_INTERMEDIATE_LAUNCHER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(NFT_INTERMEDIATE_LAUNCHER_HASH)
    }
}

impl Mod for NftRoyaltyTransferPuzzleArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH)
    }
}

impl<I, P> Mod for NftOwnershipLayerArgs<I, P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NFT_OWNERSHIP_LAYER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(NFT_OWNERSHIP_LAYER_HASH)
    }
}

impl<I, M> Mod for NftStateLayerArgs<I, M> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NFT_STATE_LAYER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(NFT_STATE_LAYER_HASH)
    }
}

impl<I> Mod for SingletonArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_TOP_LAYER_V1_1)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(SINGLETON_TOP_LAYER_V1_1_HASH)
    }
}

impl Mod for EverythingWithSignatureTailArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&EVERYTHING_WITH_SIGNATURE)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(EVERYTHING_WITH_SIGNATURE_HASH)
    }
}

impl Mod for GenesisByCoinIdTailArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&GENESIS_BY_COIN_ID)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(GENESIS_BY_COIN_ID_HASH)
    }
}
