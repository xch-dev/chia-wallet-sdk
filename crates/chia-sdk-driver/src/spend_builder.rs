use chia_protocol::Bytes32;
use chia_sdk_types::conditions::{CreateCoinWithMemos, CreateCoinWithoutMemos, ReserveFee};
use clvmr::NodePtr;

use crate::{SpendContext, SpendError};

pub trait P2Spend: Sized {
    fn raw_condition(&mut self, condition: NodePtr);

    fn reserve_fee(mut self, ctx: &mut SpendContext, fee: u64) -> Result<Self, SpendError> {
        let condition = ctx.alloc(ReserveFee { amount: fee })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn create_coin(
        mut self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(CreateCoinWithoutMemos {
            puzzle_hash,
            amount,
        })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn create_hinted_coin(
        mut self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
        amount: u64,
        hint: Bytes32,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(CreateCoinWithMemos {
            puzzle_hash,
            amount,
            memos: vec![hint.to_vec().into()],
        })?;
        self.raw_condition(condition);
        Ok(self)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChainedSpend {
    parent_conditions: Vec<NodePtr>,
}

impl ChainedSpend {
    pub fn new(parent_conditions: Vec<NodePtr>) -> Self {
        Self { parent_conditions }
    }

    pub fn extend(&mut self, other: ChainedSpend) {
        self.parent_conditions.extend(other.parent_conditions);
    }

    pub fn parent_condition(&mut self, condition: NodePtr) {
        self.parent_conditions.push(condition);
    }

    pub fn parent_conditions(&self) -> &[NodePtr] {
        &self.parent_conditions
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InnerSpend {
    puzzle: NodePtr,
    solution: NodePtr,
}

impl InnerSpend {
    pub fn new(puzzle: NodePtr, solution: NodePtr) -> Self {
        Self { puzzle, solution }
    }

    pub fn puzzle(&self) -> NodePtr {
        self.puzzle
    }

    pub fn solution(&self) -> NodePtr {
        self.solution
    }
}
