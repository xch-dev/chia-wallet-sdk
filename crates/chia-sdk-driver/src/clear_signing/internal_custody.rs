mod operations;
pub use operations::*;

use crate::{
    DriverError, InternalCustodyError, InternalCustodyLayer, InternalCustodyPolicy, Layer, Puzzle,
    SingletonLayer, SpendContext,
};
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
    /// Singleton continuation containing the resulting vault funds.
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
    /// Minimum age in seconds of the singleton input.
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

/// An immutable verified payment and signing context. Only the parser can create
/// it. Wallet transport, configuration provenance, user approval and private-key
/// storage remain the caller's responsibilities.
#[derive(Debug, Clone)]
pub struct VerifiedInternalCustodyPayment {
    operation: VerifiedInternalCustodyOperation,
}
impl VerifiedInternalCustodyPayment {
    /// Return the verified transaction effects, including the recreated singleton.
    pub fn payment(&self) -> &InternalCustodyPayment {
        let InternalCustodyOperationEffects::Payment(payment) = self.operation.effects() else {
            unreachable!()
        };
        payment
    }
    /// Frozen signing participants.
    pub fn selected_signers(&self) -> &[PublicKey] {
        self.operation.selected_signers()
    }
    /// Exact signature requirement.
    pub fn required_signature(&self) -> &RequiredBlsSignature {
        self.operation
            .required_signature()
            .expect("payment is signed")
    }
    /// Public deterministic synthetic-offset share for this signed payment.
    pub fn offset_share(&self) -> Result<Signature, DriverError> {
        self.operation
            .offset_share()?
            .ok_or_else(|| InternalCustodyError::InvalidSigningSet.into())
    }
    /// Check public synthetic-offset derivation data against the aggregate key.
    pub fn verify_synthetic_offset(&self, offset: &SecretKey) -> bool {
        let aggregate = self
            .selected_signers()
            .iter()
            .fold(PublicKey::default(), |sum, key| sum + key);
        aggregate + &offset.public_key() == self.required_signature().public_key
    }
    /// Produce a frozen participant's share after approval of `payment()`.
    pub fn sign_partial(&self, key: &SecretKey) -> Result<Signature, DriverError> {
        self.operation.sign_partial(key)
    }
    /// Verify a frozen participant's share.
    pub fn verify_partial(&self, key: &PublicKey, signature: &Signature) -> bool {
        self.operation.verify_partial(key, signature)
    }
}

/// Verify a payment and/or deposit absorption against the supplied policy and
/// caller-selected network signature-domain constants. Accepts only the singleton
/// and its bound deposit inputs. Rejects rekeys, arbitrary conditions and alternative
/// signing modes. Input programs must be canonical.
///
/// `ctx` must use an appropriately bounded allocator for untrusted inputs. CLVM
/// execution requires a positive `max_cost` shared by all inputs and uses the
/// consensus dialect directly,
/// avoiding debug dialect logs for offline signer requests. This is local proof
/// checking, not consensus validation of current chain state or relative time.
pub fn parse_internal_custody_payment(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    coin_spends: &[CoinSpend],
    selected_signers: &[PublicKey],
    constants: &AggSigConstants,
    max_cost: u64,
) -> Result<VerifiedInternalCustodyPayment, DriverError> {
    let verified = parse_internal_custody_operation(
        ctx,
        policy,
        coin_spends,
        InternalCustodyOperationRequest::Payment {
            signers: selected_signers,
        },
        constants,
        max_cost,
    )?;
    Ok(VerifiedInternalCustodyPayment {
        operation: verified,
    })
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
