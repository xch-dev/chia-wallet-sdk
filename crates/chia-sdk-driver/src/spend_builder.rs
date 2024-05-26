use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::conditions::{
    AssertBeforeHeightAbsolute, AssertBeforeHeightRelative, AssertBeforeSecondsAbsolute,
    AssertBeforeSecondsRelative, AssertCoinAnnouncement, AssertHeightAbsolute,
    AssertHeightRelative, AssertPuzzleAnnouncement, AssertSecondsAbsolute, AssertSecondsRelative,
    CreateCoin, CreateCoinAnnouncement, CreatePuzzleAnnouncement, ReserveFee,
};
use clvmr::{
    sha2::{Digest, Sha256},
    NodePtr,
};

use crate::{SpendContext, SpendError};

pub trait P2Spend: Sized {
    #[must_use]
    fn raw_condition(self, condition: NodePtr) -> Self;

    fn reserve_fee(self, ctx: &mut SpendContext<'_>, fee: u64) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&ReserveFee { amount: fee })?))
    }

    fn create_coin(
        self,
        ctx: &mut SpendContext<'_>,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&CreateCoin::new(puzzle_hash, amount))?))
    }

    fn create_hinted_coin(
        self,
        ctx: &mut SpendContext<'_>,
        puzzle_hash: Bytes32,
        amount: u64,
        hint: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&CreateCoin::with_custom_hint(
            puzzle_hash,
            amount,
            hint,
        ))?))
    }

    fn create_coin_announcement(
        self,
        ctx: &mut SpendContext<'_>,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&CreateCoinAnnouncement { message })?))
    }

    fn assert_raw_coin_announcement(
        self,
        ctx: &mut SpendContext<'_>,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertCoinAnnouncement { announcement_id })?))
    }

    fn assert_coin_announcement(
        self,
        ctx: &mut SpendContext<'_>,
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
        ctx: &mut SpendContext<'_>,
        message: Bytes,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&CreatePuzzleAnnouncement { message })?))
    }

    fn assert_raw_puzzle_announcement(
        self,
        ctx: &mut SpendContext<'_>,
        announcement_id: Bytes32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertPuzzleAnnouncement { announcement_id })?))
    }

    fn assert_puzzle_announcement(
        self,
        ctx: &mut SpendContext<'_>,
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
        ctx: &mut SpendContext<'_>,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertBeforeSecondsRelative { seconds })?))
    }

    fn assert_seconds_relative(
        self,
        ctx: &mut SpendContext<'_>,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertSecondsRelative { seconds })?))
    }

    fn assert_before_seconds_absolute(
        self,
        ctx: &mut SpendContext<'_>,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertBeforeSecondsAbsolute { seconds })?))
    }

    fn assert_seconds_absolute(
        self,
        ctx: &mut SpendContext<'_>,
        seconds: u64,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertSecondsAbsolute { seconds })?))
    }

    fn assert_before_height_relative(
        self,
        ctx: &mut SpendContext<'_>,
        height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertBeforeHeightRelative { height })?))
    }

    fn assert_height_relative(
        self,
        ctx: &mut SpendContext<'_>,
        height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertHeightRelative { height })?))
    }

    fn assert_before_height_absolute(
        self,
        ctx: &mut SpendContext<'_>,
        height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertBeforeHeightAbsolute { height })?))
    }

    fn assert_height_absolute(
        self,
        ctx: &mut SpendContext<'_>,
        height: u32,
    ) -> Result<Self, SpendError> {
        Ok(self.raw_condition(ctx.alloc(&AssertHeightAbsolute { height })?))
    }
}

#[derive(Debug, Default, Clone)]
pub struct ParentConditions {
    conditions: Vec<NodePtr>,
}

impl ParentConditions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: Self) {
        self.conditions.extend(other.conditions);
    }

    #[must_use]
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
    #[must_use]
    pub const fn new(puzzle: NodePtr, solution: NodePtr) -> Self {
        Self { puzzle, solution }
    }

    #[must_use]
    pub const fn puzzle(&self) -> NodePtr {
        self.puzzle
    }

    #[must_use]
    pub const fn solution(&self) -> NodePtr {
        self.solution
    }
}
