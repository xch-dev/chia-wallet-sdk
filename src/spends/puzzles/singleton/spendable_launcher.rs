use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::singleton::LauncherSolution;
use clvm_traits::{clvm_list, ToClvm};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{
    singleton_puzzle_hash, AssertCoinAnnouncement, ChainedSpend, SpendContext, SpendError,
};

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
        self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        key_value_list: T,
    ) -> Result<(ChainedSpend, Coin), SpendError>
    where
        T: ToClvm<NodePtr>,
    {
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

        ctx.spend(CoinSpend::new(self.coin, puzzle_reveal, solution));

        let mut chained_spend = self.chained_spend;
        chained_spend.parent_conditions.push(assert_announcement);

        let singleton_coin =
            Coin::new(self.coin.coin_id(), singleton_puzzle_hash, self.coin.amount);

        Ok((chained_spend, singleton_coin))
    }
}
