use bindy::Result;
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_driver::P2ParentCoin;
use clvm_utils::TreeHash;

use crate::Spend;

pub trait P2ParentCoinExt {
    fn inner_puzzle_hash(&self, asset_id: Option<Bytes32>) -> Result<TreeHash>;
    fn puzzle_hash(&self, asset_id: Option<Bytes32>) -> Result<TreeHash>;
    fn spend(&self, delegated_spend: Spend) -> Result<()>;
}

impl P2ParentCoinExt for P2ParentCoin {
    fn inner_puzzle_hash(&self, asset_id: Option<Bytes32>) -> Result<TreeHash> {
        Ok(P2ParentCoin::inner_puzzle_hash(asset_id))
    }

    fn puzzle_hash(&self, asset_id: Option<Bytes32>) -> Result<TreeHash> {
        Ok(P2ParentCoin::puzzle_hash(asset_id))
    }

    fn spend(&self, delegated_spend: Spend) -> Result<()> {
        let mut ctx = delegated_spend.puzzle.0.lock().unwrap();

        self.spend(
            &mut ctx,
            chia_sdk_driver::Spend::new(delegated_spend.puzzle.1, delegated_spend.solution.1),
            (),
        )?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct P2ParentCoinChildParseResult {
    pub p2_parent_coin: P2ParentCoin,
    pub memos: Vec<Bytes>,
}
