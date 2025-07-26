use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{
    puzzles::{UniquenessPrelauncher1stCurryArgs, UniquenessPrelauncher2ndCurryArgs},
    Mod,
};
use clvm_traits::ToClvm;
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Launcher, SpendContext};

#[derive(Debug, Clone)]
#[must_use]
pub struct UniquenessPrelauncher<V> {
    pub coin: Coin,
    pub value: V,
}

impl<V> UniquenessPrelauncher<V> {
    pub fn from_coin(coin: Coin, value: V) -> Self {
        Self { coin, value }
    }

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

    pub fn first_curry_hash() -> TreeHash {
        UniquenessPrelauncher1stCurryArgs {
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
        }
        .curry_tree_hash()
    }

    pub fn puzzle_hash(value_hash: TreeHash) -> TreeHash {
        CurriedProgram {
            program: Self::first_curry_hash(),
            args: UniquenessPrelauncher2ndCurryArgs { value: value_hash },
        }
        .tree_hash()
    }

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
