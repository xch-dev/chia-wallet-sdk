use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::CatalogSlotValue;

use crate::CatalogRegistryState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRegisterActionLog {
    pub spent_left_slot: CatalogSlotValue,
    pub spent_right_slot: CatalogSlotValue,
    pub created_left_slot: CatalogSlotValue,
    pub created_tail_slot: CatalogSlotValue,
    pub created_right_slot: CatalogSlotValue,
    pub prelauncher_full_puzzle_hash: Bytes32,
    pub prelauncher_id: Bytes32,
    pub launcher_id: Bytes32,
    pub registered_tail_hash: Bytes32,
    pub registered_initial_inner_puzzle_hash: Bytes32,
    pub precommit_amount: u64,
}

impl CatalogRegisterActionLog {
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        out.push(self.spent_left_slot);
        out.push(self.spent_right_slot);
    }

    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        out.push(self.created_left_slot);
        out.push(self.created_tail_slot);
        out.push(self.created_right_slot);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRefundActionLog {
    pub spent_slot: Option<CatalogSlotValue>,
    pub created_slot: Option<CatalogSlotValue>,
    pub registered_tail_hash: Bytes32,
    pub registered_initial_inner_puzzle_hash: Bytes32,
    pub precommit_amount: u64,
}

impl CatalogRefundActionLog {
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        if let Some(slot) = self.spent_slot {
            out.push(slot);
        }
    }

    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        if let Some(slot) = self.created_slot {
            out.push(slot);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogDelegatedStateActionLog {
    pub old_state: CatalogRegistryState,
    pub new_state: CatalogRegistryState,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogActionLog {
    Register(CatalogRegisterActionLog),
    Refund(CatalogRefundActionLog),
    DelegatedState(CatalogDelegatedStateActionLog),
}

impl CatalogActionLog {
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        match self {
            Self::Register(log) => log.extend_spent_slots(out),
            Self::Refund(log) => log.extend_spent_slots(out),
            Self::DelegatedState(_) => {}
        }
    }

    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        match self {
            Self::Register(log) => log.extend_created_slots(out),
            Self::Refund(log) => log.extend_created_slots(out),
            Self::DelegatedState(_) => {}
        }
    }
}
