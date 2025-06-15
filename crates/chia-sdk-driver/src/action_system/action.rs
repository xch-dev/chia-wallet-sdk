use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use hex_literal::hex;

use crate::{
    CreateDidAction, Deltas, DriverError, HashedPtr, Id, SendAction, SpendContext, Spends,
    UpdateDidAction,
};

pub const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Send(SendAction),
    CreateDid(CreateDidAction),
    UpdateDid(UpdateDidAction),
}

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(Some(id), puzzle_hash, amount, memos))
    }

    pub fn send_xch(puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(None, puzzle_hash, amount, memos))
    }

    pub fn burn(id: Id, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(Some(id), BURN_PUZZLE_HASH, amount, memos))
    }

    pub fn burn_xch(amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(None, BURN_PUZZLE_HASH, amount, memos))
    }

    pub fn create_did(
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
        amount: u64,
    ) -> Self {
        Self::CreateDid(CreateDidAction::new(
            recovery_list_hash,
            num_verifications_required,
            metadata,
            amount,
        ))
    }

    pub fn create_simple_did() -> Self {
        Self::CreateDid(CreateDidAction::default())
    }

    pub fn update_did(
        id: Id,
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
    ) -> Self {
        Self::UpdateDid(UpdateDidAction::new(
            id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
        ))
    }
}

pub trait SpendAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize);

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError>;
}

impl SpendAction for Action {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        match self {
            Action::Send(action) => action.calculate_delta(deltas, index),
            Action::CreateDid(action) => action.calculate_delta(deltas, index),
            Action::UpdateDid(action) => action.calculate_delta(deltas, index),
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        match self {
            Action::Send(action) => action.spend(ctx, spends, index),
            Action::CreateDid(action) => action.spend(ctx, spends, index),
            Action::UpdateDid(action) => action.spend(ctx, spends, index),
        }
    }
}
