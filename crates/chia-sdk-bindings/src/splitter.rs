use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver::{SplitterLayer, SplitterPayout};
use chia_sdk_types::puzzles::SplitterShare;
use clvm_utils::ToTreeHash;

use crate::{Clvm, Program, Spend};

#[derive(Clone)]
pub struct Splitter(pub(crate) SplitterLayer);

impl Splitter {
    pub fn new(shares: Vec<SplitterShare>) -> Result<Self> {
        Ok(Self(SplitterLayer::new(shares)?))
    }

    pub fn shares(&self) -> Result<Vec<SplitterShare>> {
        Ok(self.0.shares().to_vec())
    }

    pub fn total_shares(&self) -> Result<u64> {
        Ok(self.0.total_shares())
    }

    pub fn puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.0.tree_hash().into())
    }

    pub fn payouts(&self, amount: u64) -> Result<Vec<SplitterPayout>> {
        Ok(self.0.payouts(amount))
    }

    /// The coin announcement made when the coin with the given ID is split.
    pub fn announcement_id(coin_id: Bytes32) -> Result<Bytes32> {
        Ok(SplitterLayer::announcement_id(coin_id))
    }

    /// The spend of the splitter puzzle for a coin of the given amount, which can be used with
    /// either `spend_coin` or `spend_cats`.
    pub fn inner_spend(&self, clvm: Clvm, amount: u64) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let spend = self.0.inner_spend(&mut ctx, amount)?;

        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }
}
