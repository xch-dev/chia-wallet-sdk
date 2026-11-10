use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::CatalogSlotValue;

use crate::CatalogRegistryState;

/// What a [`CatalogRegisterAction`](crate::CatalogRegisterAction) spend did, parsed from its
/// solution by [`CatalogRegisterAction::get_log`](crate::CatalogRegisterAction::get_log).
///
/// The `CATalog` stores registered asset ids as a sorted linked list of slots. Registering an asset
/// id spends its two neighbors and recreates them pointing to a new slot between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRegisterActionLog {
    /// The slot to the left of the new asset id, before the spend.
    pub spent_left_slot: CatalogSlotValue,
    /// The slot to the right of the new asset id, before the spend.
    pub spent_right_slot: CatalogSlotValue,
    /// The left slot after the spend, whose right neighbor is the new asset id.
    pub created_left_slot: CatalogSlotValue,
    /// The new slot for the registered asset id.
    pub created_tail_slot: CatalogSlotValue,
    /// The right slot after the spend, whose left neighbor is the new asset id.
    pub created_right_slot: CatalogSlotValue,
    /// The puzzle hash of the [`UniquenessPrelauncher`](crate::UniquenessPrelauncher) created by
    /// the registry, which ensures the asset id can only be registered once.
    pub prelauncher_full_puzzle_hash: Bytes32,
    /// The coin id of the uniqueness prelauncher.
    pub prelauncher_id: Bytes32,
    /// The launcher id of the NFT minted for the registered asset id.
    pub launcher_id: Bytes32,
    /// The registered asset id (the tree hash of its TAIL).
    pub registered_tail_hash: Bytes32,
    /// The p2 puzzle hash of the eve NFT minted for the asset id.
    pub registered_initial_inner_puzzle_hash: Bytes32,
    /// The registration price from the registry state when the action ran, which is the amount
    /// of the precommit coin.
    pub precommit_amount: u64,
}

impl CatalogRegisterActionLog {
    /// Appends the slots spent by this action to `out`.
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        out.push(self.spent_left_slot);
        out.push(self.spent_right_slot);
    }

    /// Appends the slots created by this action to `out`.
    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        out.push(self.created_left_slot);
        out.push(self.created_tail_slot);
        out.push(self.created_right_slot);
    }
}

/// What a [`CatalogRefundAction`](crate::CatalogRefundAction) spend did, parsed from its solution
/// by [`CatalogRefundAction::get_log`](crate::CatalogRefundAction::get_log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRefundActionLog {
    /// The slot of the precommitted asset id, if the refund spent one.
    ///
    /// It's only set if the precommit coin paid the current registration price with the current
    /// CAT maker and the solution includes the slot's neighbors.
    pub spent_slot: Option<CatalogSlotValue>,
    /// The recreated slot, which is [`spent_slot`](Self::spent_slot) with its counter
    /// incremented.
    pub created_slot: Option<CatalogSlotValue>,
    /// The asset id from the refunded precommit coin.
    pub registered_tail_hash: Bytes32,
    /// The eve NFT p2 puzzle hash from the refunded precommit coin.
    pub registered_initial_inner_puzzle_hash: Bytes32,
    /// The amount of the refunded precommit coin.
    pub precommit_amount: u64,
}

impl CatalogRefundActionLog {
    /// Appends the slot spent by this action, if any, to `out`.
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        if let Some(slot) = self.spent_slot {
            out.push(slot);
        }
    }

    /// Appends the slot created by this action, if any, to `out`.
    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        if let Some(slot) = self.created_slot {
            out.push(slot);
        }
    }
}

/// What a [`DelegatedStateAction`](crate::DelegatedStateAction) spend of the `CATalog` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogDelegatedStateActionLog {
    /// The registry state before the action.
    pub old_state: CatalogRegistryState,
    /// The registry state set by the price singleton.
    pub new_state: CatalogRegistryState,
}

/// A parsed action from a `CATalog` registry spend.
///
/// The logs of a pending or parsed spend are stored in
/// [`CatalogPendingSpendInfo::logs`](crate::CatalogPendingSpendInfo::logs).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogActionLog {
    /// A [`CatalogRegisterAction`](crate::CatalogRegisterAction).
    Register(CatalogRegisterActionLog),
    /// A [`CatalogRefundAction`](crate::CatalogRefundAction).
    Refund(CatalogRefundActionLog),
    /// A [`DelegatedStateAction`](crate::DelegatedStateAction).
    DelegatedState(CatalogDelegatedStateActionLog),
}

impl CatalogActionLog {
    /// Appends the slots spent by this action to `out`.
    pub fn extend_spent_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        match self {
            Self::Register(log) => log.extend_spent_slots(out),
            Self::Refund(log) => log.extend_spent_slots(out),
            Self::DelegatedState(_) => {}
        }
    }

    /// Appends the slots created by this action to `out`.
    pub fn extend_created_slots(&self, out: &mut Vec<CatalogSlotValue>) {
        match self {
            Self::Register(log) => log.extend_created_slots(out),
            Self::Refund(log) => log.extend_created_slots(out),
            Self::DelegatedState(_) => {}
        }
    }
}
