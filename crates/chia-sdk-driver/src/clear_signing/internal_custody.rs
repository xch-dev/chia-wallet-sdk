#[cfg(test)]
use crate::{InternalCustodyLayer, Layer, Puzzle};
mod operations;
pub use operations::*;

use crate::{DriverError, InternalCustodyError, InternalCustodyPolicy, SpendContext};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::singleton::SingletonSolution;
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::{
    Condition,
    conditions::{AggSig, AggSigKind},
    puzzles::{InternalCustodyPaymentArguments, InternalCustodySpendSolution},
};
use clvm_traits::FromClvm;
use clvmr::{
    ChiaDialect, ClvmFlags, NodePtr, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

// Bound each untrusted puzzle/solution before decoding, independently of CLVM cost.
const MAX_PROGRAM_BYTES: usize = 256 * 1024;

/// Facts derived from the exact payment and/or deposit-absorption spend.
/// A positive outgoing amount creates `pending_payment`; its confirmation starts
/// the clawback period. Claim-only has no pending coin or completion step.
/// Neither this record nor the verifier establishes chain freshness/unspentness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyPayment {
    /// Singleton input authorized by this operation.
    pub input_coin: Coin,
    /// Recreated singleton containing the resulting vault funds. Every payment,
    /// absorption and no-op recreates this continuation, even when no ACH coin exists.
    pub vault_child: Coin,
    /// Outgoing ACH payment, absent for claim-only absorption.
    pub pending_payment: Option<InternalCustodyPendingPayment>,
    /// Destination committed in the signed filter, including absorb-only spends.
    pub signed_destination_context: Bytes32,
    /// Total deposited mojos absorbed by this operation.
    pub incoming_amount: u64,
    /// Whether the signed spend creates the reference empty fee announcement.
    pub fee_announcement: bool,
    /// Number of absorbed p2-singleton inputs.
    pub deposit_count: u32,
    /// Permanent identity of the custody singleton.
    pub launcher_id: Bytes32,
    /// Delay in seconds before a pending payment can be completed.
    pub clawback_seconds: u64,
    /// Minimum age in seconds since the singleton input was confirmed.
    /// Read from its executed conditions; this does not establish time readiness.
    pub withdrawal_seconds: u64,
}

/// The outgoing ACH coin and the destination committed by its puzzle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyPendingPayment {
    /// Pending coin whose confirmation starts the clawback interval.
    pub coin: Coin,
    /// Recipient puzzle hash committed to the pending coin.
    pub destination: Bytes32,
}
impl InternalCustodyPayment {
    /// Outgoing amount in mojos, or zero for claim-only absorption.
    pub fn outgoing_amount(&self) -> u64 {
        self.pending_payment.map_or(0, |p| p.coin.amount)
    }
}

// CLVM interprets zero as unlimited, so exhaustion must fail before execution.
fn run_budgeted(
    ctx: &mut SpendContext,
    puzzle: NodePtr,
    solution: NodePtr,
    remaining: &mut u64,
) -> Result<NodePtr, DriverError> {
    if *remaining == 0 {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    let reduction = run_program(
        ctx,
        &ChiaDialect::new(ClvmFlags::empty()),
        puzzle,
        solution,
        *remaining,
    )?;
    *remaining = remaining
        .checked_sub(reduction.0)
        .ok_or(InternalCustodyError::InvalidProgram)?;
    Ok(reduction.1)
}

fn canonical_program(ctx: &mut SpendContext, bytes: &[u8]) -> Result<NodePtr, DriverError> {
    if bytes.is_empty() || bytes.len() > MAX_PROGRAM_BYTES {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    let node = node_from_bytes(ctx, bytes)?;
    if node_to_bytes(ctx, node)? != bytes {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    Ok(node)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifecycle_tests;
