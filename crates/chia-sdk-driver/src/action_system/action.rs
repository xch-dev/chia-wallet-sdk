use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;

use crate::{DriverError, Id, SendAction, SpendContext, Spends};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Send(SendAction),
}

impl Action {
    pub fn send(id: Option<Id>, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(id, puzzle_hash, amount, memos))
    }

    pub fn send_xch(puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::send(None, puzzle_hash, amount, memos)
    }

    pub fn send_cat(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::send(Some(id), puzzle_hash, amount, memos)
    }
}

pub trait SpendAction {
    fn spend(&self, ctx: &mut SpendContext, spends: &mut Spends) -> Result<(), DriverError>;
}

impl SpendAction for Action {
    fn spend(&self, ctx: &mut SpendContext, spends: &mut Spends) -> Result<(), DriverError> {
        match self {
            Action::Send(action) => action.spend(ctx, spends),
        }
    }
}
