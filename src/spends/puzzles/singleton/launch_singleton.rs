use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_wallet::singleton::{
    LauncherSolution, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::{curry_tree_hash, tree_hash_atom, tree_hash_pair};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{
    AssertCoinAnnouncement, ChainedSpend, CreateCoinWithoutMemos, SpendContext, SpendError,
};

pub struct LaunchSingleton {
    coin: Coin,
}

impl LaunchSingleton {
    pub fn new(parent_coin_id: Bytes32, amount: u64) -> Self {
        Self {
            coin: Coin::new(
                parent_coin_id,
                SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                amount,
            ),
        }
    }

    pub fn coin(&self) -> &Coin {
        &self.coin
    }

    pub fn finish<T>(
        self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(ChainedSpend, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
        let create_launcher = ctx.alloc(CreateCoinWithoutMemos {
            puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            amount: self.coin.amount,
        })?;

        let singleton_puzzle_hash =
            singleton_puzzle_hash(self.coin.coin_id(), singleton_inner_puzzle_hash);

        let eve_message = ctx.alloc(clvm_list!(
            singleton_puzzle_hash,
            self.coin.amount,
            &key_value_list
        ))?;
        let eve_message_hash = ctx.tree_hash(eve_message);

        let mut announcement_id = Sha256::new();
        announcement_id.update(self.coin.coin_id());
        announcement_id.update(eve_message_hash);

        let assert_announcement = ctx.alloc(AssertCoinAnnouncement {
            announcement_id: Bytes32::new(announcement_id.finalize_fixed().into()),
        })?;

        let launcher = ctx.singleton_launcher();
        let puzzle_reveal = ctx.serialize(launcher)?;

        let solution = ctx.serialize(LauncherSolution {
            singleton_puzzle_hash,
            amount: self.coin.amount,
            key_value_list,
        })?;

        let spend_launcher = CoinSpend::new(self.coin.clone(), puzzle_reveal, solution);

        let chained_spend = ChainedSpend {
            coin_spends: vec![spend_launcher],
            parent_conditions: vec![create_launcher, assert_announcement],
        };

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((chained_spend, singleton_coin))
    }
}

pub fn singleton_puzzle_hash(launcher_id: Bytes32, inner_puzzle_hash: Bytes32) -> Bytes32 {
    let singleton_hash = tree_hash_atom(&SINGLETON_TOP_LAYER_PUZZLE_HASH);
    let launcher_id_hash = tree_hash_atom(&launcher_id);
    let launcher_puzzle_hash = tree_hash_atom(&SINGLETON_LAUNCHER_PUZZLE_HASH);

    let pair = tree_hash_pair(launcher_id_hash, launcher_puzzle_hash);
    let singleton_struct_hash = tree_hash_pair(singleton_hash, pair);

    curry_tree_hash(
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
        &[singleton_struct_hash, inner_puzzle_hash.into()],
    )
    .into()
}

#[cfg(test)]
mod tests {
    use chia_wallet::singleton::{
        SingletonArgs, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    };
    use clvm_utils::CurriedProgram;
    use clvmr::Allocator;

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

        let puzzle_hash = singleton_puzzle_hash(launcher_id, inner_puzzle_hash);

        assert_eq!(hex::encode(allocated_puzzle_hash), hex::encode(puzzle_hash));
    }
}
