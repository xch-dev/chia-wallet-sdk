use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::puzzles::{
    CompactCoinProof, XchandlesHandleSlotValue, XchandlesPricingSolution, XchandlesUpdateSlotValue,
};
use clvm_traits::FromClvm;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext, XchandlesRegistryState};

/// The revealed [`XchandlesPrecommitValue`](crate::XchandlesPrecommitValue) of a precommit coin
/// used by an `XCHandles` register, expire, or refund action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesPrecommitValueLog {
    /// The puzzle hash of the CAT maker for the payment asset.
    pub cat_maker_hash: Bytes32,
    /// The puzzle hash of the pricing puzzle.
    pub pricing_puzzle_hash: Bytes32,
    /// The solution of the pricing puzzle, which includes the handle.
    pub pricing_solution: XchandlesPricingSolution,
    /// The handle.
    pub handle: String,
    /// The secret used to hide the handle in the precommit coin's puzzle hash.
    pub secret: Bytes32,
    /// The launcher id of the singleton that owns the handle.
    pub owner_launcher_id: Bytes32,
    /// The launcher id of the singleton the handle resolves to.
    pub resolved_launcher_id: Bytes32,
}

impl XchandlesPrecommitValueLog {
    /// Creates a new [`XchandlesPrecommitValueLog`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cat_maker_hash: Bytes32,
        pricing_puzzle_hash: Bytes32,
        pricing_solution: XchandlesPricingSolution,
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Self {
        Self {
            cat_maker_hash,
            pricing_puzzle_hash,
            pricing_solution,
            handle,
            secret,
            owner_launcher_id,
            resolved_launcher_id,
        }
    }
}

#[derive(FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
struct XchandlesPricingOutput {
    total_price: u64,
    #[clvm(rest)]
    registered_time: u64,
}

/// Runs an `XCHandles` pricing puzzle with `solution` and returns `(total_price, registered_time)`.
///
/// `registered_time` is the number of seconds the handle is registered for.
///
/// # Errors
///
/// Returns an error if the puzzle fails or its output can't be parsed.
pub fn run_pricing_output(
    ctx: &mut SpendContext,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<(u64, u64), DriverError> {
    let output = ctx.run(puzzle, solution)?;
    let output = ctx.extract::<XchandlesPricingOutput>(output)?;
    Ok((output.total_price, output.registered_time))
}

/// Computes the coin id of a singleton with launcher id `owner_launcher_id` from a
/// [`CompactCoinProof`] of it.
pub fn coin_id_from_owner_proof(proof: CompactCoinProof, owner_launcher_id: Bytes32) -> Bytes32 {
    Coin::new(
        proof.parent_coin_info,
        SingletonArgs::curry_tree_hash(owner_launcher_id, proof.inner_puzzle_hash.into()).into(),
        proof.amount,
    )
    .coin_id()
}

/// What an [`XchandlesOracleAction`](crate::XchandlesOracleAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesOracleActionLog {
    /// The handle slot that was spent.
    pub spent_slot: XchandlesHandleSlotValue,
    /// The recreated handle slot, which only has its counter incremented.
    pub created_slot: XchandlesHandleSlotValue,
}

impl XchandlesOracleActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

/// What an [`XchandlesExtendAction`](crate::XchandlesExtendAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExtendActionLog {
    /// The handle slot that was spent.
    pub spent_slot: XchandlesHandleSlotValue,
    /// The recreated handle slot, with its expiration extended by
    /// [`registered_time`](Self::registered_time).
    pub created_slot: XchandlesHandleSlotValue,
    /// The price paid for the extension.
    pub total_price: u64,
    /// The number of seconds the registration was extended by.
    pub registered_time: u64,
}

impl XchandlesExtendActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

/// What an [`XchandlesExpireAction`](crate::XchandlesExpireAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExpireActionLog {
    /// The handle slot of the expired registration.
    pub spent_slot: XchandlesHandleSlotValue,
    /// The handle slot of the new registration, with the new owner and resolved launcher ids.
    pub created_slot: XchandlesHandleSlotValue,
    /// The revealed value of the precommit coin that paid for the new registration. Its
    /// pricing puzzle hash is the registry's expired handle pricing puzzle hash.
    pub precommit_value: XchandlesPrecommitValueLog,
    /// The price paid, including the premium for recently expired handles.
    pub total_price: u64,
    /// The number of seconds the handle is registered for.
    pub registered_time: u64,
    /// The full puzzle hash of the new owner singleton.
    pub owner_full_puzzle_hash: Bytes32,
    /// The full puzzle hash of the new resolved singleton, or `None` if it's the owner.
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    /// The inner puzzle hash of the new owner singleton.
    pub owner_inner_puzzle_hash: Bytes32,
    /// The inner puzzle hash of the new resolved singleton.
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesExpireActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

/// What an [`XchandlesInitiateUpdateAction`](crate::XchandlesInitiateUpdateAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesInitiateUpdateActionLog {
    /// The handle slot that was spent.
    pub spent_slot: XchandlesHandleSlotValue,
    /// The recreated handle slot, which only has its counter incremented.
    pub created_handle_slot: XchandlesHandleSlotValue,
    /// The new update slot, which stores the pending owner and resolved launcher ids.
    pub created_update_slot: XchandlesUpdateSlotValue,
    /// The coin id of the current owner singleton that initiated the update.
    pub initiator_coin_id: Bytes32,
}

impl XchandlesInitiateUpdateActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_handle_slot);
    }

    /// Appends the update slots created by this action to `out`.
    pub fn extend_created_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        out.push(self.created_update_slot);
    }
}

/// What an [`XchandlesExecuteUpdateAction`](crate::XchandlesExecuteUpdateAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExecuteUpdateActionLog {
    /// The handle slot that was spent.
    pub spent_handle_slot: XchandlesHandleSlotValue,
    /// The update slot that was spent.
    pub spent_update_slot: XchandlesUpdateSlotValue,
    /// The recreated handle slot, with the new owner and resolved launcher ids.
    pub created_slot: XchandlesHandleSlotValue,
    /// The coin id of the current owner singleton, whose parent initiated the update.
    pub owner_coin_id: Bytes32,
    /// The full puzzle hash of the new owner singleton.
    pub owner_full_puzzle_hash: Bytes32,
    /// The full puzzle hash of the new resolved singleton, or `None` if it's the owner.
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    /// The inner puzzle hash of the new owner singleton.
    pub owner_inner_puzzle_hash: Bytes32,
    /// The inner puzzle hash of the new resolved singleton.
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesExecuteUpdateActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_handle_slot);
    }

    /// Appends the update slots spent by this action to `out`.
    pub fn extend_spent_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        out.push(self.spent_update_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_slot);
    }
}

/// What an [`XchandlesRefundAction`](crate::XchandlesRefundAction) spend did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRefundActionLog {
    /// The handle slot spent by the refund, if any.
    pub spent_slot: Option<XchandlesHandleSlotValue>,
    /// The recreated handle slot, which is [`spent_slot`](Self::spent_slot) with its counter
    /// incremented.
    pub created_slot: Option<XchandlesHandleSlotValue>,
    /// The revealed value of the refunded precommit coin.
    pub precommit_value: XchandlesPrecommitValueLog,
    /// The price computed by the precommitted pricing puzzle and solution.
    pub precommitted_total_price: u64,
    /// The registration time computed by the precommitted pricing puzzle and solution.
    pub precommitted_registered_time: u64,
}

impl XchandlesRefundActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        if let Some(slot) = self.spent_slot {
            out.push(slot);
        }
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        if let Some(slot) = self.created_slot {
            out.push(slot);
        }
    }
}

/// What an [`XchandlesRegisterAction`](crate::XchandlesRegisterAction) spend did.
///
/// The registry stores handle hashes as a sorted linked list of slots. Registering a handle
/// spends its two neighbors and recreates them pointing to a new slot between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesRegisterActionLog {
    /// The slot to the left of the new handle, before the spend.
    pub spent_left_slot: XchandlesHandleSlotValue,
    /// The slot to the right of the new handle, before the spend.
    pub spent_right_slot: XchandlesHandleSlotValue,
    /// The left slot after the spend, whose right neighbor is the new handle.
    pub created_left_slot: XchandlesHandleSlotValue,
    /// The new slot for the registered handle.
    pub created_handle_slot: XchandlesHandleSlotValue,
    /// The right slot after the spend, whose left neighbor is the new handle.
    pub created_right_slot: XchandlesHandleSlotValue,
    /// The revealed value of the precommit coin, including the handle, secret, and pricing.
    pub precommit_value: XchandlesPrecommitValueLog,
    /// The price paid for the registration.
    pub total_price: u64,
    /// The number of seconds the handle is registered for.
    pub registered_time: u64,
    /// The full puzzle hash of the owner singleton.
    pub owner_full_puzzle_hash: Bytes32,
    /// The full puzzle hash of the resolved singleton, or `None` if it's the owner.
    pub resolved_full_puzzle_hash: Option<Bytes32>,
    /// The inner puzzle hash of the owner singleton.
    pub owner_inner_puzzle_hash: Bytes32,
    /// The inner puzzle hash of the resolved singleton.
    pub resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesRegisterActionLog {
    /// Appends the handle slots spent by this action to `out`.
    pub fn extend_spent_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.spent_left_slot);
        out.push(self.spent_right_slot);
    }

    /// Appends the handle slots created by this action to `out`.
    pub fn extend_created_handle_slots(&self, out: &mut Vec<XchandlesHandleSlotValue>) {
        out.push(self.created_left_slot);
        out.push(self.created_handle_slot);
        out.push(self.created_right_slot);
    }
}

/// What a [`DelegatedStateAction`](crate::DelegatedStateAction) spend of the `XCHandles` registry
/// did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesDelegatedStateActionLog {
    /// The registry state before the action.
    pub old_state: XchandlesRegistryState,
    /// The registry state set by the price singleton.
    pub new_state: XchandlesRegistryState,
}

impl XchandlesDelegatedStateActionLog {}

/// A parsed action from an `XCHandles` registry spend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum XchandlesActionLog {
    /// An [`XchandlesOracleAction`](crate::XchandlesOracleAction).
    Oracle(XchandlesOracleActionLog),
    /// An [`XchandlesExtendAction`](crate::XchandlesExtendAction).
    Extend(XchandlesExtendActionLog),
    /// An [`XchandlesExpireAction`](crate::XchandlesExpireAction).
    Expire(XchandlesExpireActionLog),
    /// An [`XchandlesInitiateUpdateAction`](crate::XchandlesInitiateUpdateAction).
    InitiateUpdate(XchandlesInitiateUpdateActionLog),
    /// An [`XchandlesExecuteUpdateAction`](crate::XchandlesExecuteUpdateAction).
    ExecuteUpdate(XchandlesExecuteUpdateActionLog),
    /// An [`XchandlesRefundAction`](crate::XchandlesRefundAction).
    Refund(XchandlesRefundActionLog),
    /// An [`XchandlesRegisterAction`](crate::XchandlesRegisterAction).
    Register(XchandlesRegisterActionLog),
    /// A [`DelegatedStateAction`](crate::DelegatedStateAction).
    DelegatedState(XchandlesDelegatedStateActionLog),
}

impl XchandlesActionLog {
    /// Appends the handle slots spent by this action to `out`.
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

    /// Appends the handle slots created by this action to `out`.
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

    /// Appends the update slots spent by this action to `out`.
    pub fn extend_spent_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        if let Self::ExecuteUpdate(log) = self {
            log.extend_spent_update_slots(out);
        }
    }

    /// Appends the update slots created by this action to `out`.
    pub fn extend_created_update_slots(&self, out: &mut Vec<XchandlesUpdateSlotValue>) {
        if let Self::InitiateUpdate(log) = self {
            log.extend_created_update_slots(out);
        }
    }
}
