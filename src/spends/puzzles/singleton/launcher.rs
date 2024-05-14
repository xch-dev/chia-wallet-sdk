use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use clvm_utils::{curry_tree_hash, tree_hash_atom, tree_hash_pair};

use crate::{ChainedSpend, CreateCoinWithoutMemos, SpendContext, SpendError, SpendableLauncher};

pub struct Launcher {
    coin: Coin,
}

impl Launcher {
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self {
            coin: Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
        }
    }

    pub fn coin(&self) -> Coin {
        self.coin
    }

    pub fn create(self, ctx: &mut SpendContext) -> Result<SpendableLauncher, SpendError> {
        let amount = self.coin.amount;

        Ok(SpendableLauncher::new(
            self.coin,
            ChainedSpend {
                parent_conditions: vec![ctx.alloc(CreateCoinWithoutMemos {
                    puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    amount,
                })?],
            },
        ))
    }
}

pub fn singleton_struct_hash(launcher_id: Bytes32) -> Bytes32 {
    let singleton_hash = tree_hash_atom(&SINGLETON_TOP_LAYER_PUZZLE_HASH);
    let launcher_id_hash = tree_hash_atom(&launcher_id);
    let launcher_puzzle_hash = tree_hash_atom(&SINGLETON_LAUNCHER_PUZZLE_HASH);

    let pair = tree_hash_pair(launcher_id_hash, launcher_puzzle_hash);
    tree_hash_pair(singleton_hash, pair).into()
}

pub fn singleton_puzzle_hash(launcher_id: Bytes32, inner_puzzle_hash: Bytes32) -> Bytes32 {
    curry_tree_hash(
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
        &[
            singleton_struct_hash(launcher_id).into(),
            inner_puzzle_hash.into(),
        ],
    )
    .into()
}

#[cfg(test)]
mod tests {
    use chia_puzzles::singleton::{
        SingletonArgs, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    };
    use clvm_utils::CurriedProgram;
    use clvmr::Allocator;

    use crate::SpendContext;

    use super::*;

    #[test]
    fn test_puzzle_hash() {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let inner_puzzle = ctx.alloc([1, 2, 3]).unwrap();
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle);

        let launcher_id = Bytes32::new([34; 32]);

        let singleton_puzzle = ctx.singleton_top_layer();

        let puzzle = ctx
            .alloc(CurriedProgram {
                program: singleton_puzzle,
                args: SingletonArgs {
                    singleton_struct: SingletonStruct {
                        mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                        launcher_id,
                        launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    },
                    inner_puzzle,
                },
            })
            .unwrap();
        let allocated_puzzle_hash = ctx.tree_hash(puzzle);

        let puzzle_hash = singleton_puzzle_hash(launcher_id, inner_puzzle_hash.into());

        assert_eq!(hex::encode(allocated_puzzle_hash), hex::encode(puzzle_hash));
    }
}
