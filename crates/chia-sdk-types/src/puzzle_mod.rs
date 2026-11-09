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

/// A puzzle "mod" (the uncurried program) that is known at compile time, along with its hash.
///
/// In Chia, many puzzles are built by currying arguments into a shared program. The program
/// before currying is called the mod, and its tree hash is the mod hash. This trait gives access
/// to the serialized mod ([`Mod::mod_reveal`]) and its hash ([`Mod::mod_hash`]).
///
/// This trait should be implemented for types that represent the curried arguments of puzzles,
/// which by convention end in `Args` and derive `ToClvm` and `FromClvm` with `#[clvm(curry)]`.
/// However, if a puzzle can't be curried (it has no arguments), this trait can still be
/// implemented on a marker struct that doesn't implement [`ToClvm`], such as
/// [`SettlementPayment`](crate::puzzles::SettlementPayment). This disables the
/// [`curry_tree_hash`](Mod::curry_tree_hash) method.
///
/// It's implemented for the common puzzle argument types from [`chia_puzzle_types`] (such as
/// [`StandardArgs`], [`CatArgs`], and [`SingletonArgs`]), as well as the puzzle argument types in
/// [`puzzles`](crate::puzzles). Use the [`compile_chialisp!`](macro@crate::compile_chialisp) or
/// [`compile_rue!`](macro@crate::compile_rue) macros to implement it for your own puzzles.
///
/// ## Usage Example
///
/// We can specify the arguments of a puzzle to get its curried puzzle hash, without having to
/// allocate the puzzle. This matches the tree hash of the actual curried puzzle.
///
/// ```rust
/// # use chia_bls::PublicKey;
/// # use chia_puzzle_types::standard::StandardArgs;
/// # use chia_sdk_types::Mod;
/// # use clvm_traits::ToClvm;
/// # use clvm_utils::{CurriedProgram, tree_hash};
/// # use clvmr::{Allocator, serde::node_from_bytes};
/// let args = StandardArgs::new(PublicKey::default());
/// let puzzle_hash = args.curry_tree_hash();
///
/// let mut allocator = Allocator::new();
/// let program = node_from_bytes(&mut allocator, &StandardArgs::mod_reveal())?;
/// let puzzle = CurriedProgram { program, args }.to_clvm(&mut allocator)?;
///
/// assert_eq!(tree_hash(&allocator, program), StandardArgs::mod_hash());
/// assert_eq!(tree_hash(&allocator, puzzle), puzzle_hash);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub trait Mod {
    /// The serialized CLVM bytecode of the uncurried puzzle.
    fn mod_reveal() -> Cow<'static, [u8]>;

    /// The tree hash of the uncurried puzzle.
    fn mod_hash() -> TreeHash;

    /// Calculates the puzzle hash of the mod with these arguments curried in.
    ///
    /// This only hashes the arguments and combines them with [`Mod::mod_hash`], which is much
    /// faster than allocating and hashing the whole curried puzzle.
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
