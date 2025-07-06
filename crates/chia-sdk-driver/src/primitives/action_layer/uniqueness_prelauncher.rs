use chia::{
    clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes32, Coin, CoinSpend},
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_wallet_sdk::driver::{DriverError, Launcher, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::SpendContextExt;

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
        CurriedProgram {
            program: UNIQUENESS_PRELAUNCHER_PUZZLE_HASH,
            args: UniquenessPrelauncher1stCurryArgs {
                launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
            },
        }
        .tree_hash()
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
        let program = ctx.uniqueness_prelauncher_puzzle()?;
        let prog_1st_curry = ctx.alloc(&CurriedProgram {
            program,
            args: UniquenessPrelauncher1stCurryArgs {
                launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
            },
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

pub const UNIQUENESS_PRELAUNCHER_PUZZLE: [u8; 59] = hex!("ff02ffff01ff04ffff04ff04ffff04ff05ffff01ff01808080ffff04ffff04ff06ffff04ff0bff808080ff808080ffff04ffff01ff333eff018080");

pub const UNIQUENESS_PRELAUNCHER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    851c3d39cef84cfd9449afcaeff5f50d1be9371d8b7d6057ac318bec553a1a9f
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher1stCurryArgs {
    pub launcher_puzzle_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct UniquenessPrelauncher2ndCurryArgs<V> {
    pub value: V,
}
