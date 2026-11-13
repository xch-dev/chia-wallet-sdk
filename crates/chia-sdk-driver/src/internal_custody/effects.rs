use crate::{InternalCustodyInfo, InternalCustodyThresholds};
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};

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

/// Policy definition independently checked against the committed root.
#[derive(Debug, Clone)]
pub struct InternalCustodyPolicyDefinition {
    /// Singleton identity and immutable configuration, including its new root.
    pub info: InternalCustodyInfo,
    /// Keys in the definition.
    pub keys: Vec<PublicKey>,
    /// Signing/recovery threshold ladder.
    pub thresholds: InternalCustodyThresholds,
}
/// Exact verified lifecycle effects. Chain existence, spentness, confirmation and
/// time readiness remain the caller's responsibility, including for staged locks.
#[derive(Debug, Clone)]
pub enum InternalCustodyOperationEffects {
    /// Payment, absorption, or a no-op, including its singleton continuation.
    Payment(InternalCustodyPayment),
    /// ACH cancellation creates an unclaimed deposit, not singleton funds.
    PaymentClawback {
        /// Recipient committed by the cancelled pending payment.
        destination: Bytes32,
        /// Whether this signed spend creates the empty fee-sponsor announcement.
        fee_announcement: bool,
        /// Consumed ACH input.
        pending: Coin,
        /// Exact returned p2-singleton coin.
        refund: Coin,
        /// Permanent vault identity.
        launcher_id: Bytes32,
    },
    /// ACH completion creates the committed recipient coin.
    PaymentCompletion {
        /// Consumed ACH input.
        pending: Coin,
        /// Exact payout coin.
        payout: Coin,
    },
    /// A signed singleton spend proposes a new root.
    Rekey {
        /// Whether this signed spend creates the empty fee-sponsor announcement.
        fee_announcement: bool,
        /// Consumed singleton.
        input: Coin,
        /// Old-policy continuation.
        vault_child: Coin,
        /// Zero-value rekey input for completion/cancellation.
        pending: Coin,
        /// Verified target definition.
        new_policy: InternalCustodyPolicyDefinition,
        /// Recovery delay multiple.
        timelock_multiple: u8,
        /// Minimum age in seconds since the initiating singleton was confirmed.
        /// Read from the executed singleton conditions, not claimed to have elapsed.
        initiation_seconds: u64,
        /// Minimum age of the pending coin before completion, in seconds.
        completion_seconds: u64,
    },
    /// Cancel a zero-value pending attempt; no vault funds move.
    RekeyClawback {
        /// Whether this signed spend creates the empty fee-sponsor announcement.
        fee_announcement: bool,
        /// Consumed pending coin.
        pending: Coin,
        /// Target root that is being cancelled.
        new_root: Bytes32,
        /// Exact attempt's multiple.
        timelock_multiple: u8,
    },
    /// A confirmed pending attempt authorizes the current singleton's root change.
    RekeyCompletion {
        /// Consumed singleton.
        input: Coin,
        /// Consumed pending attempt.
        pending: Coin,
        /// Exact replacement singleton.
        vault_child: Coin,
        /// Verified target policy.
        new_policy: InternalCustodyPolicyDefinition,
    },
    /// Higher-threshold authorization to the precommitted next root. These effects
    /// describe the two dependent stages, not a claim that they are confirmed.
    IncreaseSecurity {
        /// First-stage singleton input.
        input: Coin,
        /// First-stage continuation.
        intermediate_singleton: Coin,
        /// Zero-value lock coin.
        pending: Coin,
        /// Final singleton after unsigned stage two.
        vault_child: Coin,
        /// Verified next-level definition.
        new_policy: InternalCustodyPolicyDefinition,
    },
}
