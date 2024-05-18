use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::conditions::*;
use clvmr::NodePtr;
use sha2::{Digest, Sha256};

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

    fn create_coin_announcement(
        mut self,
        ctx: &mut SpendContext,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(CreateCoinAnnouncement { message })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn assert_raw_coin_announcement(
        mut self,
        ctx: &mut SpendContext,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(AssertCoinAnnouncement { announcement_id })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn assert_coin_announcement(
        self,
        ctx: &mut SpendContext,
        coin_id: Bytes32,
        message: impl AsRef<[u8]>,
    ) -> Result<Self, SpendError> {
        let mut announcement_id = Sha256::new();
        announcement_id.update(coin_id);
        announcement_id.update(message);
        self.assert_raw_coin_announcement(ctx, Bytes32::new(announcement_id.finalize().into()))
    }

    fn create_puzzle_announcement(
        mut self,
        ctx: &mut SpendContext,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(CreatePuzzleAnnouncement { message })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn assert_raw_puzzle_announcement(
        mut self,
        ctx: &mut SpendContext,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        let condition = ctx.alloc(AssertPuzzleAnnouncement { announcement_id })?;
        self.raw_condition(condition);
        Ok(self)
    }

    fn assert_puzzle_announcement(
        self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
        message: impl AsRef<[u8]>,
    ) -> Result<Self, SpendError> {
        let mut announcement_id = Sha256::new();
        announcement_id.update(puzzle_hash);
        announcement_id.update(message);
        self.assert_raw_coin_announcement(ctx, Bytes32::new(announcement_id.finalize().into()))
    }
}

#[derive(Debug, Default, Clone)]
pub struct ParentConditions {
    conditions: Vec<NodePtr>,
}

impl ParentConditions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: ParentConditions) {
        self.conditions.extend(other.conditions);
    }

    pub fn parent_conditions(&self) -> &[NodePtr] {
        &self.conditions
    }
}

impl P2Spend for ParentConditions {
    fn raw_condition(&mut self, condition: NodePtr) {
        self.conditions.push(condition);
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
