use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{
    Mod,
    puzzles::{UniquenessPrelauncher1stCurryArgs, UniquenessPrelauncher2ndCurryArgs},
};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Launcher, SpendContext};

/// A 0-amount coin that commits to a value and creates a singleton launcher.
///
/// When spent, the prelauncher creates a puzzle announcement of its value and a launcher coin
/// with amount 1. Since the value is curried into its puzzle, the resulting launcher id is unique
/// to the combination of the prelauncher's parent and the value. The
/// [`CatalogRegistry`](crate::CatalogRegistry) uses this to launch one NFT per registered CAT.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{SpendContext, UniquenessPrelauncher};
///
/// let ctx = &mut SpendContext::new();
///
/// let parent_coin_id = Bytes32::new([1; 32]);
/// let tail_hash = Bytes32::new([2; 32]);
///
/// let prelauncher = UniquenessPrelauncher::<Bytes32>::new(ctx, parent_coin_id, tail_hash)?;
/// assert_eq!(prelauncher.coin.amount, 0);
///
/// let prelauncher_id = prelauncher.coin.coin_id();
/// let launcher = prelauncher.spend(ctx)?;
/// assert_eq!(launcher.coin().parent_coin_info, prelauncher_id);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct UniquenessPrelauncher<V> {
    /// The prelauncher coin.
    pub coin: Coin,
    /// The value curried into the prelauncher puzzle.
    pub value: V,
}

impl<V> UniquenessPrelauncher<V> {
    /// Creates a [`UniquenessPrelauncher`] from an existing coin and its value, without checking the puzzle hash.
    pub fn from_coin(coin: Coin, value: V) -> Self {
        Self { coin, value }
    }

    /// Creates the 0-amount prelauncher coin for `value` with the given parent coin id.
    pub fn new(
        allocator: &mut Allocator,
        parent_coin_id: Bytes32,
        value: V,
    ) -> Result<Self, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let value_ptr = value.to_clvm(allocator)?;
        let value_hash = tree_hash(allocator, value_ptr);

        Ok(Self::from_coin(
            Coin::new(
                parent_coin_id,
                UniquenessPrelauncher::<V>::puzzle_hash(value_hash).into(),
                0,
            ),
            value,
        ))
    }

    /// Returns the tree hash of the prelauncher puzzle curried with only the singleton launcher puzzle hash.
    pub fn first_curry_hash() -> TreeHash {
        UniquenessPrelauncher1stCurryArgs {
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
        .curry_tree_hash()
    }

    /// Returns the prelauncher puzzle hash for a value with the given tree hash.
    pub fn puzzle_hash(value_hash: TreeHash) -> TreeHash {
        CurriedProgram {
            program: Self::first_curry_hash(),
            args: UniquenessPrelauncher2ndCurryArgs { value: value_hash },
        }
        .tree_hash()
    }

    /// Allocates the prelauncher puzzle.
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let prog_1st_curry = ctx.curry(UniquenessPrelauncher1stCurryArgs {
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        })?;

        ctx.alloc(&CurriedProgram {
            program: prog_1st_curry,
            args: UniquenessPrelauncher2ndCurryArgs {
                value: self.value.clone(),
            },
        })
    }

    /// Spends the prelauncher and returns the [`Launcher`] it creates, which has an amount of 1.
    ///
    /// The prelauncher coin must exist on-chain or be created in the same spend bundle.
    pub fn spend(self, ctx: &mut SpendContext) -> Result<Launcher, DriverError>
    where
        V: ToClvm<Allocator> + Clone,
    {
        let puzzle_reveal = self.construct_puzzle(ctx)?;
        let puzzle_reveal = ctx.serialize(&puzzle_reveal)?;

        let solution = ctx.serialize(&NodePtr::NIL)?;

        ctx.insert(CoinSpend::new(self.coin, puzzle_reveal, solution));

        Ok(Launcher::new(self.coin.coin_id(), 1))
    }
}
