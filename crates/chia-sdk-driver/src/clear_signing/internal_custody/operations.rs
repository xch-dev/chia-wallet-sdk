//! Exact construction-backed verification for Internal Custody lifecycle operations.
use super::{
    AggSig, AggSigConstants, AggSigKind, Bytes32, Coin, CoinSpend, Condition, DriverError,
    FromClvm, InternalCustodyError, InternalCustodyPolicy, InternalCustodySpendSolution, NodePtr,
    PublicKey, RequiredBlsSignature, SecretKey, Signature, SingletonSolution, SpendContext,
    canonical_program, run_budgeted,
};
use crate::{InternalCustodyInfo, InternalCustodyThresholds};
use chia_puzzle_types::{DeriveSynthetic, LineageProof};
use chia_sdk_types::Mod;
use chia_sdk_types::puzzles::{
    InternalCustodyMerkleSolution, InternalCustodyRekeyArguments, InternalCustodyRekeyState,
};

/// Caller-selected operation definition. None of these fields are trusted review
/// facts: every supplied spend must exactly match its canonical reconstruction.
#[derive(Debug, Clone, Copy)]
pub enum InternalCustodyOperationRequest<'a> {
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
        /// Proposed policy committed by the input.
        new_policy: &'a InternalCustodyPolicy,
        /// Frozen participants for this attempt.
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
    /// ACH cancellation creates an unclaimed deposit, not singleton funds.
    PaymentClawback {
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
    },
    /// Cancel a zero-value pending attempt; no vault funds move.
    RekeyClawback {
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
    /// Frozen participant subset (empty for unsigned completion).
    pub fn selected_signers(&self) -> &[PublicKey] {
        &self.selected_signers
    }
    /// Required condition, absent for unsigned completion.
    pub fn required_signature(&self) -> Option<&RequiredBlsSignature> {
        self.signature.as_ref()
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
/// Verify a lifecycle operation against exact typed reconstruction and a shared,
/// positive CLVM execution budget. This checks local proofs, not live chain state.
pub fn parse_internal_custody_operation(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    request: InternalCustodyOperationRequest<'_>,
    constants: &AggSigConstants,
    max_cost: u64,
) -> Result<VerifiedInternalCustodyOperation, DriverError> {
    use InternalCustodyOperationEffects as E;
    use InternalCustodyOperationRequest as R;
    let first = spends
        .first()
        .ok_or(InternalCustodyError::UnsupportedOperation)?;
    if spends.len() > 3 {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    }
    let (expected, signers, effects, expected_key, raw_message_len) = match request {
        R::PaymentClawback {
            destination,
            signers,
        } => {
            let expected = match_fee_form(
                |fee| {
                    policy
                        .payment_clawback_spend(ctx, first.coin, destination, signers, fee)
                        .map(|s| vec![s])
                },
                spends,
            )?;
            let refund = Coin::new(
                first.coin.coin_id(),
                chia_sdk_types::puzzles::P2SingletonArgs::new(policy.info().launcher_id)
                    .curry_tree_hash()
                    .into(),
                first.coin.amount,
            );
            (
                expected,
                signers.to_vec(),
                E::PaymentClawback {
                    pending: first.coin,
                    refund,
                    launcher_id: policy.info().launcher_id,
                },
                Some(policy.selected_key(signers)?),
                32,
            )
        }
        R::PaymentCompletion { destination } => {
            let expected = vec![policy.completion_spend(ctx, first.coin, destination)?];
            (
                expected,
                vec![],
                E::PaymentCompletion {
                    pending: first.coin,
                    payout: Coin::new(first.coin.coin_id(), destination, first.coin.amount),
                },
                None,
                0,
            )
        }
        R::Rekey {
            new_policy,
            signers,
        } => {
            validate_target(ctx, policy, new_policy)?;
            let raw = singleton_solution(ctx, first)?;
            let expected = match_fee_form(
                |fee| {
                    policy
                        .start_rekey_spend(
                            ctx,
                            first.coin,
                            raw.lineage_proof,
                            new_policy,
                            signers,
                            fee,
                        )
                        .map(|s| vec![s])
                },
                spends,
            )?;
            let multiple = policy.rekey_multiple(signers)?;
            let state = InternalCustodyRekeyState {
                new_root: new_policy.info().puzzle_root,
                old_root: policy.info().puzzle_root,
                timelock_multiple: multiple,
            };
            let puzzle = policy.pending_rekey_puzzle(ctx, state)?;
            let pending = Coin::new(first.coin.coin_id(), ctx.tree_hash(puzzle).into(), 0);
            (
                expected,
                signers.to_vec(),
                E::Rekey {
                    input: first.coin,
                    vault_child: Coin::new(
                        first.coin.coin_id(),
                        first.coin.puzzle_hash,
                        first.coin.amount,
                    ),
                    pending,
                    new_policy: definition(new_policy),
                    timelock_multiple: multiple,
                },
                Some(policy.aggregate_key(signers)?.derive_synthetic()),
                32,
            )
        }
        R::RekeyClawback {
            new_policy,
            signers,
        } => {
            validate_target(ctx, policy, new_policy)?;
            let expected = match_fee_form(
                |fee| {
                    policy
                        .rekey_clawback_spend(ctx, first.coin, new_policy, signers, fee)
                        .map(|s| vec![s])
                },
                spends,
            )?;
            (
                expected,
                signers.to_vec(),
                E::RekeyClawback {
                    pending: first.coin,
                    new_root: new_policy.info().puzzle_root,
                    timelock_multiple: policy.rekey_multiple(signers)?,
                },
                Some(policy.aggregate_key(signers)?.derive_synthetic()),
                32,
            )
        }
        R::RekeyCompletion { new_policy } => {
            validate_target(ctx, policy, new_policy)?;
            if spends.len() != 2 {
                return Err(InternalCustodyError::UnsupportedOperation.into());
            }
            let raw = singleton_solution(ctx, first)?;
            let action =
                InternalCustodyRekeyArguments::from_clvm(&**ctx, raw.inner_solution.action)?;
            if action.new_root != new_policy.info().puzzle_root {
                return Err(InternalCustodyError::InvalidPolicy.into());
            }
            let node = canonical_program(ctx, spends[1].solution.as_ref())?;
            let pending =
                InternalCustodyMerkleSolution::<NodePtr, NodePtr>::from_clvm(&**ctx, node)?;
            let (lineage, ()): (LineageProof, ()) = FromClvm::from_clvm(&**ctx, pending.solution)?;
            let state = InternalCustodyRekeyState {
                new_root: action.new_root,
                old_root: policy.info().puzzle_root,
                timelock_multiple: action.timelock_multiple,
            };
            let expected = policy
                .complete_rekey_spends(
                    ctx,
                    first.coin,
                    spends[1].coin,
                    state,
                    (raw.lineage_proof, lineage),
                )?
                .to_vec();
            let child = new_policy.singleton_puzzle(ctx)?;
            (
                expected,
                vec![],
                E::RekeyCompletion {
                    input: first.coin,
                    pending: spends[1].coin,
                    vault_child: Coin::new(
                        first.coin.coin_id(),
                        ctx.tree_hash(child).into(),
                        first.coin.amount,
                    ),
                    new_policy: definition(new_policy),
                },
                None,
                0,
            )
        }
        R::IncreaseSecurity {
            new_policy,
            signers,
        } => {
            validate_target(ctx, policy, new_policy)?;
            if policy.next_root() != Some(new_policy.info().puzzle_root) {
                return Err(InternalCustodyError::InvalidPolicy.into());
            }
            let raw = singleton_solution(ctx, first)?;
            let stages =
                policy.increase_security_spends(ctx, first.coin, raw.lineage_proof, signers)?;
            let pending = stages.completion[0].coin;
            let intermediate_singleton = stages.completion[1].coin;
            let mut expected = vec![stages.authorization];
            if spends.len() != 1 {
                expected.extend(stages.completion);
            }
            let child = new_policy.singleton_puzzle(ctx)?;
            (
                expected,
                signers.to_vec(),
                E::IncreaseSecurity {
                    input: first.coin,
                    intermediate_singleton,
                    pending,
                    vault_child: Coin::new(
                        intermediate_singleton.coin_id(),
                        ctx.tree_hash(child).into(),
                        first.coin.amount,
                    ),
                    new_policy: definition(new_policy),
                },
                Some(policy.aggregate_key(signers)?),
                0,
            )
        }
    };
    if expected != spends {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    let mut remaining = max_cost;
    let mut signature = None;
    let mut coin_announcements = std::collections::HashSet::new();
    let mut puzzle_announcements = std::collections::HashSet::new();
    let mut coin_assertions = Vec::new();
    let mut puzzle_assertions = Vec::new();
    for spend in spends {
        let puzzle = canonical_program(ctx, spend.puzzle_reveal.as_ref())?;
        let solution = canonical_program(ctx, spend.solution.as_ref())?;
        let output = run_budgeted(ctx, puzzle, solution, &mut remaining)?;
        for condition in Vec::<Condition>::from_clvm(&**ctx, output)? {
            match condition {
                Condition::AggSigMe(c)
                    if signature.is_none()
                        && Some(c.public_key) == expected_key
                        && c.message.len() == raw_message_len =>
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
                Condition::AggSigMe(_)
                | Condition::AggSigUnsafe(_)
                | Condition::AggSigParent(_)
                | Condition::AggSigPuzzle(_)
                | Condition::AggSigAmount(_)
                | Condition::AggSigPuzzleAmount(_)
                | Condition::AggSigParentAmount(_)
                | Condition::AggSigParentPuzzle(_) => {
                    return Err(InternalCustodyError::UnexpectedConditions.into());
                }
                Condition::AssertMyParentId(c) if c.parent_id != spend.coin.parent_coin_info => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::AssertMyAmount(c) if c.amount != spend.coin.amount => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::AssertMyPuzzleHash(c) if c.puzzle_hash != spend.coin.puzzle_hash => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::CreateCoinAnnouncement(c) => {
                    coin_announcements.insert(chia_sdk_types::announcement_id(
                        spend.coin.coin_id(),
                        c.message,
                    ));
                }
                Condition::CreatePuzzleAnnouncement(c) => {
                    puzzle_announcements.insert(chia_sdk_types::announcement_id(
                        spend.coin.puzzle_hash,
                        c.message,
                    ));
                }
                Condition::AssertCoinAnnouncement(c) => coin_assertions.push(c.announcement_id),
                Condition::AssertPuzzleAnnouncement(c) => puzzle_assertions.push(c.announcement_id),
                _ => (), // Canonical exact reconstruction fixes all remaining effects/conditions.
            }
        }
    }
    if coin_assertions
        .iter()
        .any(|id| !coin_announcements.contains(id))
        || puzzle_assertions
            .iter()
            .any(|id| !puzzle_announcements.contains(id))
    {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    if signature.is_some() != expected_key.is_some() {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    Ok(VerifiedInternalCustodyOperation {
        effects,
        selected_signers: signers,
        signature,
    })
}
fn validate_target(
    ctx: &mut SpendContext,
    old: &InternalCustodyPolicy,
    new: &InternalCustodyPolicy,
) -> Result<(), DriverError> {
    old.check_update(new)?;
    new.verify_definition(ctx)
}
fn definition(policy: &InternalCustodyPolicy) -> InternalCustodyPolicyDefinition {
    InternalCustodyPolicyDefinition {
        info: policy.info(),
        keys: policy.keys().to_vec(),
        thresholds: policy.thresholds(),
    }
}
fn singleton_solution(
    ctx: &mut SpendContext,
    spend: &CoinSpend,
) -> Result<
    SingletonSolution<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>>,
    DriverError,
> {
    let solution = canonical_program(ctx, spend.solution.as_ref())?;
    Ok(FromClvm::from_clvm(&**ctx, solution)?)
}
fn match_fee_form(
    mut build: impl FnMut(bool) -> Result<Vec<CoinSpend>, DriverError>,
    spends: &[CoinSpend],
) -> Result<Vec<CoinSpend>, DriverError> {
    for fee in [false, true] {
        let expected = build(fee)?;
        if expected == spends {
            return Ok(expected);
        }
    }
    Err(InternalCustodyError::UnexpectedConditions.into())
}
