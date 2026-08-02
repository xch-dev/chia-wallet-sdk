use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::puzzles::{
    CompactCoinProof, XchandlesHandleSlotValue, XchandlesPricingSolution, XchandlesUpdateSlotValue,
};
use clvm_traits::FromClvm;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext, XchandlesPrecommitValue, XchandlesRegistryState};

pub type XchandlesPrecommitValueLog =
    XchandlesPrecommitValue<(), XchandlesPricingSolution, Bytes32>;

#[derive(FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
struct XchandlesPricingOutput {
    total_price: u64,
    #[clvm(rest)]
    registered_time: u64,
}

pub fn run_pricing_output(
    ctx: &mut SpendContext,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<(u64, u64), DriverError> {
    let output = ctx.run(puzzle, solution)?;
    let output = ctx.extract::<XchandlesPricingOutput>(output)?;
    Ok((output.total_price, output.registered_time))
}

pub fn coin_id_from_owner_proof(proof: CompactCoinProof, owner_launcher_id: Bytes32) -> Bytes32 {
    Coin::new(
        proof.parent_coin_info,
        SingletonArgs::curry_tree_hash(owner_launcher_id, proof.inner_puzzle_hash.into()).into(),
        proof.amount,
    )
    .coin_id()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesOracleActionLog {
    pub spent_slot: XchandlesHandleSlotValue,
    pub created_slot: XchandlesHandleSlotValue,
}

impl XchandlesOracleActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExtendActionLog {
    pub spent_slot: XchandlesHandleSlotValue,
    pub created_slot: XchandlesHandleSlotValue,
    pub total_price: u64,
    pub registered_time: u64,
}

impl XchandlesExtendActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExpireActionLog {
    pub spent_slot: XchandlesHandleSlotValue,
    pub created_slot: XchandlesHandleSlotValue,
    pub precommit_value: XchandlesPrecommitValueLog,
    pub total_price: u64,
    pub registered_time: u64,
    pub owner_full_puzzle_hash: Bytes32,
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    pub owner_inner_puzzle_hash: Bytes32,
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesExpireActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesInitiateUpdateActionLog {
    pub spent_slot: XchandlesHandleSlotValue,
    pub created_handle_slot: XchandlesHandleSlotValue,
    pub created_update_slot: XchandlesUpdateSlotValue,
    pub initiator_coin_id: Bytes32,
}

impl XchandlesInitiateUpdateActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_handle_slot);
    }

    pub fn extend_created_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        out.push(self.created_update_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExecuteUpdateActionLog {
    pub spent_handle_slot: XchandlesHandleSlotValue,
    pub spent_update_slot: XchandlesUpdateSlotValue,
    pub created_slot: XchandlesHandleSlotValue,
    pub owner_coin_id: Bytes32,
    pub owner_full_puzzle_hash: Bytes32,
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    pub owner_inner_puzzle_hash: Bytes32,
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesExecuteUpdateActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_handle_slot);
    }

    pub fn extend_spent_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        out.push(self.spent_update_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRefundActionLog {
    pub spent_slot: Option<XchandlesHandleSlotValue>,
    pub created_slot: Option<XchandlesHandleSlotValue>,
    pub precommit_value: XchandlesPrecommitValueLog,
    pub precommitted_total_price: u64,
    pub precommitted_registered_time: u64,
}

impl XchandlesRefundActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        if let Some(slot) = self.spent_slot {
            out.push(slot);
        }
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        if let Some(slot) = self.created_slot {
            out.push(slot);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRegisterActionLog {
    pub spent_left_slot: XchandlesHandleSlotValue,
    pub spent_right_slot: XchandlesHandleSlotValue,
    pub created_left_slot: XchandlesHandleSlotValue,
    pub created_handle_slot: XchandlesHandleSlotValue,
    pub created_right_slot: XchandlesHandleSlotValue,
    /// Complete revealed precommit (Handle string + registration secret + pricing).
    pub precommit_value: XchandlesPrecommitValueLog,
    pub total_price: u64,
    pub registered_time: u64,
    pub owner_full_puzzle_hash: Bytes32,
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    pub owner_inner_puzzle_hash: Bytes32,
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesRegisterActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_left_slot);
        out.push(self.spent_right_slot);
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_left_slot);
        out.push(self.created_handle_slot);
        out.push(self.created_right_slot);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesDelegatedStateActionLog {
    pub old_state: XchandlesRegistryState,
    pub new_state: XchandlesRegistryState,
}

impl XchandlesDelegatedStateActionLog {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum XchandlesActionLog {
    Oracle(XchandlesOracleActionLog),
    Extend(XchandlesExtendActionLog),
    Expire(XchandlesExpireActionLog),
    InitiateUpdate(XchandlesInitiateUpdateActionLog),
    ExecuteUpdate(XchandlesExecuteUpdateActionLog),
    Refund(XchandlesRefundActionLog),
    Register(XchandlesRegisterActionLog),
    DelegatedState(XchandlesDelegatedStateActionLog),
}

impl XchandlesActionLog {
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        match self {
            Self::Oracle(log) => log.extend_spent_handle_slots(out),
            Self::Extend(log) => log.extend_spent_handle_slots(out),
            Self::Expire(log) => log.extend_spent_handle_slots(out),
            Self::InitiateUpdate(log) => log.extend_spent_handle_slots(out),
            Self::ExecuteUpdate(log) => log.extend_spent_handle_slots(out),
            Self::Refund(log) => log.extend_spent_handle_slots(out),
            Self::Register(log) => log.extend_spent_handle_slots(out),
            Self::DelegatedState(_log) => {} // no slots spent
        }
    }

    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        match self {
            Self::Oracle(log) => log.extend_created_handle_slots(out),
            Self::Extend(log) => log.extend_created_handle_slots(out),
            Self::Expire(log) => log.extend_created_handle_slots(out),
            Self::InitiateUpdate(log) => log.extend_created_handle_slots(out),
            Self::ExecuteUpdate(log) => log.extend_created_handle_slots(out),
            Self::Refund(log) => log.extend_created_handle_slots(out),
            Self::Register(log) => log.extend_created_handle_slots(out),
            Self::DelegatedState(_log) => {} // no slots created
        }
    }

    pub fn extend_spent_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        if let Self::ExecuteUpdate(log) = self {
            log.extend_spent_update_slots(out);
        }
    }

    pub fn extend_created_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        if let Self::InitiateUpdate(log) = self {
            log.extend_created_update_slots(out);
        }
    }
}
