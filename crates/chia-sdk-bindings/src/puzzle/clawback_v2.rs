use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver::ClawbackV2;
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{Clvm, Program, Spend};

pub trait ClawbackV2Ext: Sized {
    fn from_memo(
        memo: Program,
        receiver_puzzle_hash: Bytes32,
        amount: u64,
        hinted: bool,
        expected_puzzle_hash: Bytes32,
    ) -> Result<Option<Self>>;
    fn memo(&self, clvm: Clvm) -> Result<Program>;
    fn sender_spend(&self, spend: Spend) -> Result<Spend>;
    fn receiver_spend(&self, spend: Spend) -> Result<Spend>;
    fn push_through_spend(&self, clvm: Clvm) -> Result<Spend>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl ClawbackV2Ext for ClawbackV2 {
    fn from_memo(
        memo: Program,
        receiver_puzzle_hash: Bytes32,
        amount: u64,
        hinted: bool,
        expected_puzzle_hash: Bytes32,
    ) -> Result<Option<Self>> {
        let ctx = memo.0.lock().unwrap();
        Ok(Self::from_memo(
            &ctx,
            memo.1,
            receiver_puzzle_hash,
            amount,
            hinted,
            expected_puzzle_hash,
        ))
    }

    fn memo(&self, clvm: Clvm) -> Result<Program> {
        let mut ctx = clvm.0.lock().unwrap();
        let ptr = self.memo().to_clvm(&mut **ctx)?;
        Ok(Program(clvm.0.clone(), ptr))
    }

    fn sender_spend(&self, spend: Spend) -> Result<Spend> {
        let ctx_clone = spend.puzzle.0.clone();
        let mut ctx = ctx_clone.lock().unwrap();
        let spend = self.sender_spend(&mut ctx, spend.into())?;
        Ok(Spend {
            puzzle: Program(ctx_clone.clone(), spend.puzzle),
            solution: Program(ctx_clone.clone(), spend.solution),
        })
    }

    fn receiver_spend(&self, spend: Spend) -> Result<Spend> {
        let ctx_clone = spend.puzzle.0.clone();
        let mut ctx = ctx_clone.lock().unwrap();
        let spend = self.receiver_spend(&mut ctx, spend.into())?;
        Ok(Spend {
            puzzle: Program(ctx_clone.clone(), spend.puzzle),
            solution: Program(ctx_clone.clone(), spend.solution),
        })
    }

    fn push_through_spend(&self, clvm: Clvm) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let spend = self.push_through_spend(&mut ctx)?;
        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.tree_hash())
    }
}
