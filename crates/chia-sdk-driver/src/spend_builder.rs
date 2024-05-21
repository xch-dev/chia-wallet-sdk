use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::conditions::*;
use clvmr::{
    sha2::{Digest, Sha256},
    NodePtr,
};

use crate::{SpendContext, SpendError};

pub trait P2Spend: Sized {
    fn raw_condition(self, condition: NodePtr) -> Self;

    fn reserve_fee(self, ctx: &mut SpendContext, fee: u64) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(ReserveFee { amount: fee })?))
    }

    fn create_coin(
        self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(CreateCoinWithoutMemos {
            puzzle_hash,
            amount,
        })?))
    }

    fn create_hinted_coin(
        self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
        amount: u64,
        hint: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(CreateCoinWithMemos {
            puzzle_hash,
            amount,
            memos: vec![hint.to_vec().into()],
        })?))
    }

    fn create_coin_announcement(
        self,
        ctx: &mut SpendContext,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(CreateCoinAnnouncement { message })?))
    }

    fn assert_raw_coin_announcement(
        self,
        ctx: &mut SpendContext,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertCoinAnnouncement { announcement_id })?))
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
        self,
        ctx: &mut SpendContext,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(CreatePuzzleAnnouncement { message })?))
    }

    fn assert_raw_puzzle_announcement(
        self,
        ctx: &mut SpendContext,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertPuzzleAnnouncement { announcement_id })?))
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

    fn assert_before_seconds_relative(
        self,
        ctx: &mut SpendContext,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertBeforeSecondsRelative { seconds })?))
    }

    fn assert_seconds_relative(
        self,
        ctx: &mut SpendContext,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertSecondsRelative { seconds })?))
    }

    fn assert_before_seconds_absolute(
        self,
        ctx: &mut SpendContext,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertBeforeSecondsAbsolute { seconds })?))
    }

    fn assert_seconds_absolute(
        self,
        ctx: &mut SpendContext,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertSecondsAbsolute { seconds })?))
    }

    fn assert_before_height_relative(
        self,
        ctx: &mut SpendContext,
        block_height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertBeforeHeightRelative { block_height })?))
    }

    fn assert_height_relative(
        self,
        ctx: &mut SpendContext,
        block_height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertHeightRelative { block_height })?))
    }

    fn assert_before_height_absolute(
        self,
        ctx: &mut SpendContext,
        block_height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertBeforeHeightAbsolute { block_height })?))
    }

    fn assert_height_absolute(
        self,
        ctx: &mut SpendContext,
        block_height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(AssertHeightAbsolute { block_height })?))
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
    fn raw_condition(mut self, condition: NodePtr) -> Self {
        self.conditions.push(condition);
        self
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
