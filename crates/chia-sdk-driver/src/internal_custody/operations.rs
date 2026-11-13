use super::{
    execution::{ExpectedAuthorization, canonical_program, execute},
    reconstruction::reconstruct,
};
use crate::{
    DriverError, InternalCustodyError, InternalCustodyOperationEffects, InternalCustodyPayment,
    InternalCustodyPolicy, SpendContext,
};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes32, CoinSpend};
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::puzzles::InternalCustodyRekeyState;

/// Caller-selected operation definition. None of these fields are trusted review
/// facts: every supplied spend must exactly match its canonical reconstruction.
#[derive(Debug, Clone, Copy)]
pub enum InternalCustodyOperationRequest<'a> {
    /// Verify the exact payment action and fee form decoded from the signed input.
    Payment {
        /// Frozen signing participants.
        signers: &'a [PublicKey],
    },
    /// Cancel the ACH coin committed to this recipient.
    PaymentClawback {
        /// Original recipient.
        destination: Bytes32,
        /// Frozen signing participants.
        signers: &'a [PublicKey],
    },
    /// Complete an ACH coin after its relative delay.
    PaymentCompletion {
        /// Committed recipient.
        destination: Bytes32,
    },
    /// Start a normal rekey or slow recovery.
    Rekey {
        /// Independently derived replacement policy.
        new_policy: &'a InternalCustodyPolicy,
        /// Frozen signing participants.
        signers: &'a [PublicKey],
    },
    /// Cancel a pending ordinary/slow rekey.
    RekeyClawback {
        /// Exact pending root and delay, without requiring the target roster.
        state: InternalCustodyRekeyState,
        /// Frozen participants whose minimum delay must not exceed this attempt's.
        signers: &'a [PublicKey],
    },
    /// Finish a normal/slow rekey or a confirmed lock stage.
    RekeyCompletion {
        /// Policy committed by the pending input.
        new_policy: &'a InternalCustodyPolicy,
    },
    /// Verify stage one alone, or Python's complete three-spend lock bundle.
    IncreaseSecurity {
        /// The independently derived next-level policy.
        new_policy: &'a InternalCustodyPolicy,
        /// Higher-threshold participants.
        signers: &'a [PublicKey],
    },
}
impl<'a> InternalCustodyOperationRequest<'a> {
    fn authorization(
        self,
        policy: &InternalCustodyPolicy,
    ) -> Result<(&'a [PublicKey], ExpectedAuthorization), DriverError> {
        match self {
            Self::Payment { signers }
            | Self::PaymentClawback { signers, .. }
            | Self::Rekey { signers, .. }
            | Self::RekeyClawback { signers, .. } => Ok((
                signers,
                ExpectedAuthorization::Standard(policy.aggregate_key(signers)?),
            )),
            Self::IncreaseSecurity { signers, .. } => Ok((
                signers,
                ExpectedAuthorization::Lock(policy.aggregate_key(signers)?),
            )),
            Self::PaymentCompletion { .. } | Self::RekeyCompletion { .. } => {
                Ok((&[], ExpectedAuthorization::Unsigned))
            }
        }
    }
}
/// Immutable operation verification and optional exact signature requirement.
#[derive(Debug, Clone)]
pub struct VerifiedInternalCustodyOperation {
    effects: InternalCustodyOperationEffects,
    selected_signers: Vec<PublicKey>,
    signature: Option<RequiredBlsSignature>,
}
impl VerifiedInternalCustodyOperation {
    /// Facts proven by construction and execution.
    pub fn effects(&self) -> &InternalCustodyOperationEffects {
        &self.effects
    }
    /// Payment facts, present only for payment/absorption/no-op operations.
    pub fn payment(&self) -> Option<&InternalCustodyPayment> {
        match &self.effects {
            InternalCustodyOperationEffects::Payment(payment) => Some(payment),
            _ => None,
        }
    }
    /// Frozen participant subset (empty for unsigned completion).
    pub fn selected_signers(&self) -> &[PublicKey] {
        &self.selected_signers
    }
    /// Required condition, absent for unsigned completion.
    pub fn required_signature(&self) -> Option<&RequiredBlsSignature> {
        self.signature.as_ref()
    }
    /// Public deterministic synthetic-offset signature share, absent for raw-key
    /// lock authorization and unsigned completion. This contains no custodian key.
    pub fn offset_share(&self) -> Result<Option<Signature>, DriverError> {
        let Some(required) = &self.signature else {
            return Ok(None);
        };
        let aggregate = self
            .selected_signers
            .iter()
            .fold(PublicKey::default(), |sum, key| sum + key);
        if aggregate == required.public_key {
            return Ok(None);
        }
        // chia_puzzle_types keeps its synthetic_offset helper private.
        let mut hash = chia_sha2::Sha256::new();
        hash.update(aggregate.to_bytes());
        hash.update(chia_puzzle_types::standard::DEFAULT_HIDDEN_PUZZLE_HASH);
        let offset = SecretKey::from_bytes(&chia_puzzle_types::mod_by_group_order(hash.finalize()))
            .map_err(|_| InternalCustodyError::InvalidSigningSet)?;
        if aggregate + &offset.public_key() != required.public_key {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(Some(required.sign_partial(&offset)?))
    }
    /// Produce a share after the caller obtains approval of these exact effects.
    pub fn sign_partial(&self, key: &SecretKey) -> Result<Signature, DriverError> {
        if !self.selected_signers.contains(&key.public_key()) {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(self
            .signature
            .as_ref()
            .ok_or(InternalCustodyError::UnsupportedOperation)?
            .sign_partial(key)?)
    }
    /// Verify a frozen participant's share in the exact condition context.
    pub fn verify_partial(&self, key: &PublicKey, signature: &Signature) -> bool {
        self.selected_signers.contains(key)
            && self
                .signature
                .as_ref()
                .is_some_and(|s| s.verify_partial(key, signature))
    }
}
/// Verify an operation by canonical reconstruction, budgeted execution, then
/// construction of immutable effects. This checks local proofs, not live chain state.
pub fn parse_internal_custody_operation(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    request: InternalCustodyOperationRequest<'_>,
    constants: &AggSigConstants,
    max_cost: u64,
) -> Result<VerifiedInternalCustodyOperation, DriverError> {
    if spends.is_empty() || spends.len() > crate::MAX_INTERNAL_CUSTODY_INPUTS {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    }
    let decoded = spends
        .iter()
        .map(|spend| {
            Ok((
                canonical_program(ctx, spend.puzzle_reveal.as_ref())?,
                canonical_program(ctx, spend.solution.as_ref())?,
            ))
        })
        .collect::<Result<Vec<_>, DriverError>>()?;
    let (signers, authorization) = request.authorization(policy)?;
    // Reject unsupported spends before consuming the CLVM execution budget.
    let reconstructed = reconstruct(ctx, policy, spends, request, &decoded)?;
    let execution = execute(ctx, spends, &decoded, &authorization, constants, max_cost)?;
    let effects = reconstructed.describe(ctx, policy, spends, &execution)?;
    Ok(VerifiedInternalCustodyOperation {
        effects,
        selected_signers: signers.to_vec(),
        signature: execution.signature,
    })
}
