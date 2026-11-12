mod operations;
pub use operations::*;

use crate::{
    DriverError, InternalCustodyError, InternalCustodyLayer, InternalCustodyPolicy, Layer, Puzzle,
    SingletonLayer, SpendContext,
};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{Memos, singleton::SingletonSolution};
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
    payment: InternalCustodyPayment,
    selected_signers: Vec<PublicKey>,
    signature: RequiredBlsSignature,
}
impl VerifiedInternalCustodyPayment {
    /// Return the verified transaction effects.
    pub fn payment(&self) -> &InternalCustodyPayment {
        &self.payment
    }
    /// Return the exact frozen signing subset.
    pub fn selected_signers(&self) -> &[PublicKey] {
        &self.selected_signers
    }
    /// Return the aggregate-key signature condition extracted from execution.
    pub fn required_signature(&self) -> &RequiredBlsSignature {
        &self.signature
    }

    /// Check a public synthetic-offset hint against the verified aggregate key.
    /// The offset is public derivation data, not a custodian's private key.
    pub fn verify_synthetic_offset(&self, offset: &SecretKey) -> bool {
        let aggregate = self
            .selected_signers
            .iter()
            .fold(PublicKey::default(), |sum, key| sum + key);
        aggregate + &offset.public_key() == self.signature.public_key
    }
    /// Produce a share only for a participant in the verified frozen subset.
    /// Callers must obtain user approval of `payment()` before accessing the key.
    pub fn sign_partial(&self, key: &SecretKey) -> Result<Signature, DriverError> {
        if !self.selected_signers.contains(&key.public_key()) {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(self.signature.sign_partial(key)?)
    }
    /// Verify a share only for a participant in the verified subset.
    pub fn verify_partial(&self, participant: &PublicKey, signature: &Signature) -> bool {
        self.selected_signers.contains(participant)
            && self.signature.verify_partial(participant, signature)
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
    let Some(spend) = coin_spends
        .first()
        .filter(|_| coin_spends.len() <= crate::MAX_INTERNAL_CUSTODY_INPUTS)
    else {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    };
    let puzzle = canonical_program(ctx, spend.puzzle_reveal.as_ref())?;
    let solution = canonical_program(ctx, spend.solution.as_ref())?;
    if ctx.tree_hash(puzzle) != spend.coin.puzzle_hash.into() {
        return Err(DriverError::WrongPuzzleHash);
    }
    let Some(layer) =
        SingletonLayer::<InternalCustodyLayer>::parse_puzzle(ctx, Puzzle::parse(ctx, puzzle))?
    else {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    };
    if layer.launcher_id != policy.info().launcher_id
        || layer.inner_puzzle != policy.inner_layer(ctx)?
    {
        return Err(InternalCustodyError::InvalidPolicy.into());
    }
    let singleton =
        SingletonSolution::<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr>>::from_clvm(
            &**ctx, solution,
        )?;
    if singleton.inner_solution.spend_type != 2 {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    }
    let action =
        InternalCustodyPaymentArguments::from_clvm(&**ctx, singleton.inner_solution.action)?;
    let deposits: Vec<_> = coin_spends[1..].iter().map(|s| s.coin).collect();
    let payment_action = if action.outgoing > 0 {
        crate::InternalCustodyPaymentAction::Withdraw(crate::InternalCustodyWithdrawal {
            amount: action.outgoing,
            destination: action.destination,
        })
    } else {
        crate::InternalCustodyPaymentAction::Absorb {
            destination_context: action.destination,
        }
    };
    // The reference CLI's sole extra condition is an empty fee announcement.
    // Require an exact reconstruction of one of these two explicit forms.
    let mut matched = None;
    for fee_announcement in [false, true] {
        let expected = policy.payment_spends_with_options(
            ctx,
            spend.coin,
            singleton.lineage_proof,
            crate::InternalCustodyPaymentOptions {
                action: payment_action,
                fee_announcement,
            },
            selected_signers,
            &deposits,
        )?;
        if expected == coin_spends {
            matched = Some(fee_announcement);
            break;
        }
    }
    let fee_announcement = matched.ok_or(InternalCustodyError::UnexpectedConditions)?;
    let final_key = policy.selected_key(selected_signers)?;
    let info = policy.info();
    let pending_puzzle = policy.pending_payment_puzzle(ctx, action.destination)?;
    let payment = InternalCustodyPayment {
        input_coin: spend.coin,
        vault_child: Coin::new(
            spend.coin.coin_id(),
            spend.coin.puzzle_hash,
            crate::internal_custody_remaining_amount(
                spend.coin.amount,
                action.incoming,
                action.outgoing,
            )?,
        ),
        incoming_amount: action.incoming,
        fee_announcement,
        deposit_count: u32::try_from(deposits.len())
            .map_err(|_| InternalCustodyError::InvalidPayment)?,
        pending_payment: (action.outgoing > 0).then(|| InternalCustodyPendingPayment {
            destination: action.destination,
            coin: Coin::new(
                spend.coin.coin_id(),
                ctx.tree_hash(pending_puzzle).into(),
                action.outgoing,
            ),
        }),
        launcher_id: info.launcher_id,
        clawback_seconds: info.payment_clawback,
        withdrawal_seconds: info.withdrawal_timelock,
    };
    // One shared execution budget for the entire request, including deposits.
    let mut cost_left = max_cost;
    for deposit in &coin_spends[1..] {
        let puzzle = canonical_program(ctx, deposit.puzzle_reveal.as_ref())?;
        let solution = canonical_program(ctx, deposit.solution.as_ref())?;
        run_budgeted(ctx, puzzle, solution, &mut cost_left)?;
    }
    let output = run_budgeted(ctx, puzzle, solution, &mut cost_left)?;
    let conditions = Vec::<Condition>::from_clvm(&**ctx, output)?;
    let mut signature = None;
    let mut outputs = Vec::new();
    let mut parents = 0;
    let mut amounts = 0;
    let mut timelocks = 0;
    let mut announcements = Vec::new();
    let mut assertions = Vec::new();
    let mut fee_announcements = 0;
    for condition in conditions {
        match condition {
            Condition::AggSigMe(c)
                if signature.is_none() && c.public_key == final_key && c.message.len() == 32 =>
            {
                signature = Some(RequiredBlsSignature::try_from_condition(
                    &spend.coin,
                    AggSig {
                        kind: AggSigKind::Me,
                        public_key: c.public_key,
                        message: c.message,
                    },
                    constants,
                )?);
            }
            Condition::CreateCoin(c) if c.memos == Memos::<NodePtr>::None => {
                outputs.push(Coin::new(spend.coin.coin_id(), c.puzzle_hash, c.amount));
            }
            Condition::AssertMyParentId(c) if c.parent_id == spend.coin.parent_coin_info => {
                parents += 1;
            }
            Condition::AssertMyAmount(c) if c.amount == spend.coin.amount => {
                amounts += 1;
            }
            Condition::AssertSecondsRelative(c) if c.seconds == info.withdrawal_timelock => {
                timelocks += 1;
            }
            Condition::CreateCoinAnnouncement(c)
                if fee_announcement && fee_announcements == 0 && c.message.is_empty() =>
            {
                fee_announcements += 1;
            }
            Condition::CreatePuzzleAnnouncement(c) => announcements.push(c.message),
            Condition::AssertCoinAnnouncement(c) => assertions.push(c.announcement_id),
            Condition::AssertMyParentId(_) | Condition::AssertMyAmount(_) => {
                return Err(InternalCustodyError::CoinAssertion.into());
            }
            _ => return Err(InternalCustodyError::UnexpectedConditions.into()),
        }
    }
    let expected_outputs: Vec<_> = std::iter::once(payment.vault_child)
        .chain(payment.pending_payment.map(|p| p.coin))
        .collect();
    let expected_announcements: Vec<chia_protocol::Bytes> =
        deposits.iter().map(|c| c.coin_id().into()).collect();
    let expected_assertions: Vec<Bytes32> = deposits
        .iter()
        .map(|c| crate::deposit_announcement(c.coin_id()).into())
        .collect();
    if announcements != expected_announcements || assertions != expected_assertions {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    if fee_announcements != usize::from(fee_announcement)
        || parents != 1
        || amounts != 2
        || timelocks != 1
        || outputs != expected_outputs
    {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    Ok(VerifiedInternalCustodyPayment {
        payment,
        selected_signers: selected_signers.to_vec(),
        signature: signature.ok_or(InternalCustodyError::UnexpectedConditions)?,
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
