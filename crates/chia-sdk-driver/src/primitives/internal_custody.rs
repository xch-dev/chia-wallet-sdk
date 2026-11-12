//! Construction and public policy data for the legacy prefarm custody puzzles.
mod authorization;
mod clawback;
mod rekey;
pub use rekey::InternalCustodyLockSpends;
mod payment;
mod policy;
pub use policy::*;

/// Invalid or unsupported legacy custody data. The clear-signing parser supports
/// only payments and deposit absorption, never a generic delegated spend fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InternalCustodyError {
    /// Invalid internal-custody policy.
    #[error("invalid internal-custody policy")]
    InvalidPolicy,
    /// Invalid internal-custody signing subset.
    #[error("invalid internal-custody signing subset")]
    InvalidSigningSet,
    /// Unsupported internal-custody operation.
    #[error("unsupported internal-custody operation")]
    UnsupportedOperation,
    /// Invalid internal-custody payment.
    #[error("invalid internal-custody payment")]
    InvalidPayment,
    /// Noncanonical or oversized internal-custody program.
    #[error("noncanonical or oversized internal-custody program")]
    InvalidProgram,
    /// Internal-custody coin assertion or lineage mismatch.
    #[error("internal-custody coin assertion or lineage mismatch")]
    CoinAssertion,
    /// Internal-custody conditions do not match the verified payment.
    #[error("internal-custody conditions do not match the verified payment")]
    UnexpectedConditions,
}

pub(crate) use payment::deposit_announcement;

/// Maximum supported singleton-plus-deposit inputs in a verified custody operation.
/// This bounds reconstruction and per-input execution work alongside the cost budget.
pub const MAX_INTERNAL_CUSTODY_INPUTS: usize = 64;

/// An outgoing payment to create alongside the vault continuation.
/// Use `None` in `InternalCustodyPolicy::payment_spends` for claim-only absorption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyWithdrawal {
    /// Positive, even payment amount in mojos.
    pub amount: u64,
    /// Destination puzzle hash released by ACH completion.
    pub destination: chia_protocol::Bytes32,
}

/// Payment action, including the context retained by Python for absorb-only spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalCustodyPaymentAction {
    /// Create a pending ACH payment.
    Withdraw(InternalCustodyWithdrawal),
    /// Recreate the singleton without an outgoing coin. The context is signed
    /// but is not a recipient when no payment output exists.
    Absorb {
        /// Legacy signed destination context.
        destination_context: chia_protocol::Bytes32,
    },
}
/// Explicit reference-compatible payment construction options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyPaymentOptions {
    /// Reviewed fund movement.
    pub action: InternalCustodyPaymentAction,
    /// Append the CLI's empty `CREATE_COIN_ANNOUNCEMENT` for external fee funding.
    pub fee_announcement: bool,
}
