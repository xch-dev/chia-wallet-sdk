use bindy::Result;
use chia_sdk_driver::Clawback;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{Clvm, Program, Remark, Spend};

pub trait ClawbackExt: Sized {
    fn get_remark_condition(&self, clvm: Clvm) -> Result<Remark>;
    fn sender_spend(&self, spend: Spend) -> Result<Spend>;
    fn receiver_spend(&self, spend: Spend) -> Result<Spend>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl ClawbackExt for Clawback {
    fn get_remark_condition(&self, clvm: Clvm) -> Result<Remark> {
        let mut ctx = clvm.0.lock().unwrap();
        let ptr = self.get_remark_condition(&mut ctx)?.rest;
        Ok(Remark {
            rest: Program(clvm.0.clone(), ptr),
        })
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

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.to_layer().tree_hash())
    }
}
