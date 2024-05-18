use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::singleton::{
    LauncherSolution, SingletonArgs, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE,
    SINGLETON_TOP_LAYER_PUZZLE_HASH,
};
use chia_sdk_types::conditions::AssertCoinAnnouncement;
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{spend_builder::ChainedSpend, SpendContext, SpendError};

#[must_use = "Launcher coins must be spent in order to create the singleton output."]
pub struct SpendableLauncher {
    coin: Coin,
    chained_spend: ChainedSpend,
}

impl SpendableLauncher {
    pub(crate) fn new(coin: Coin, chained_spend: ChainedSpend) -> Self {
        Self {
            coin,
            chained_spend,
        }
    }

    pub fn coin(&self) -> Coin {
        self.coin
    }

    pub fn spend<T>(
        mut self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(ChainedSpend, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
        let singleton_puzzle_hash = CurriedProgram {
            program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
            args: SingletonArgs {
                singleton_struct: SingletonStruct::new(self.coin.coin_id()),
                inner_puzzle: TreeHash::from(singleton_inner_puzzle_hash),
            },
        }
        .tree_hash()
        .into();

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

        let solution = ctx.serialize(LauncherSolution {
            singleton_puzzle_hash,
            amount: self.coin.amount,
            key_value_list,
        })?;

        ctx.spend(CoinSpend::new(
            self.coin,
            Program::from(SINGLETON_LAUNCHER_PUZZLE.to_vec()),
            solution,
        ));

        self.chained_spend.parent_condition(assert_announcement);

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((self.chained_spend, singleton_coin))
    }
}
