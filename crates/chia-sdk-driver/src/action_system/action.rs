use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;

use crate::{
    CreateDidAction, Deltas, DriverError, HashedPtr, Id, SendAction, SpendContext, Spends,
};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Send(SendAction),
    CreateDid(CreateDidAction),
}

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(Some(id), puzzle_hash, amount, memos))
    }

    pub fn send_xch(puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(None, puzzle_hash, amount, memos))
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
        }
    }
}
