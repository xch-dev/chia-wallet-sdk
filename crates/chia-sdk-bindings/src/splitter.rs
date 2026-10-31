use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver as sdk;
use chia_sdk_types::puzzles::SplitterShare;

use crate::{Clvm, Program, Spend};

#[derive(Clone)]
pub struct Splitter {
    pub shares: Vec<SplitterShare>,
}

impl Splitter {
    pub fn new(shares: Vec<SplitterShare>) -> Result<Self> {
        Ok(sdk::Splitter::new(shares)?.into())
    }

    fn to_sdk(&self) -> Result<sdk::Splitter> {
        Ok(sdk::Splitter::new(self.shares.clone())?)
    }

    pub fn total_shares(&self) -> Result<u64> {
        Ok(self.to_sdk()?.total_shares())
    }

    pub fn puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.to_sdk()?.puzzle_hash().into())
    }

    pub fn payouts(&self, amount: u64) -> Result<Vec<sdk::SplitterPayout>> {
        Ok(self.to_sdk()?.payouts(amount))
    }

    /// The coin announcement made when the coin with the given ID is split.
    pub fn announcement_id(coin_id: Bytes32) -> Result<Bytes32> {
        Ok(sdk::Splitter::announcement_id(coin_id))
    }

    /// The spend of the splitter puzzle for a coin of the given amount, which can be used with
    /// either `spend_coin` or `spend_cats`.
    pub fn inner_spend(&self, clvm: Clvm, amount: u64) -> Result<Spend> {
        let splitter = self.to_sdk()?;
        let mut ctx = clvm.0.lock().unwrap();
        let spend = splitter.inner_spend(&mut ctx, amount)?;

        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }
}

impl From<sdk::Splitter> for Splitter {
    fn from(value: sdk::Splitter) -> Self {
        Self {
            shares: value.shares().to_vec(),
        }
    }
}
