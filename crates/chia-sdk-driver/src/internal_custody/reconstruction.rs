mod payment;
use super::{
    effects::{InternalCustodyOperationEffects as E, InternalCustodyPolicyDefinition},
    execution::Execution,
    operations::InternalCustodyOperationRequest as R,
};
use crate::{DriverError, InternalCustodyError, InternalCustodyPolicy, SpendContext};
use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzle_types::{LineageProof, Proof, singleton::SingletonSolution};
use chia_sdk_types::{
    Mod,
    puzzles::{
        InternalCustodyMerkleSolution, InternalCustodyRekeyArguments, InternalCustodyRekeyState,
        InternalCustodySpendSolution,
    },
};
use clvm_traits::FromClvm;
use clvmr::NodePtr;
use payment::{ReconstructedPayment, reconstruct_payment};

pub(super) enum Reconstruction<'a> {
    Payment(ReconstructedPayment),
    Rekey {
        state: InternalCustodyRekeyState,
        proof: Proof,
        new_policy: &'a InternalCustodyPolicy,
        fee_announcement: bool,
    },
    Ready(Box<E>),
}
impl Reconstruction<'_> {
    fn ready(effects: E) -> Self {
        Self::Ready(Box::new(effects))
    }
    pub fn describe(
        self,
        ctx: &mut SpendContext,
        policy: &InternalCustodyPolicy,
        spends: &[CoinSpend],
        execution: &Execution,
    ) -> Result<E, DriverError> {
        match self {
            Self::Ready(effects) => Ok(*effects),
            Self::Payment(payment) => Ok(E::Payment(payment.describe(
                ctx,
                policy,
                spends,
                execution.relative_seconds[0],
            )?)),
            Self::Rekey {
                state,
                proof,
                new_policy,
                fee_announcement,
            } => {
                let input = spends[0].coin;
                let puzzle = policy.pending_rekey_puzzle(ctx, state)?;
                Ok(E::Rekey {
                    fee_announcement,
                    input,
                    vault_child: crate::Singleton::new(input, proof, policy)
                        .child_with(policy, input.amount)
                        .coin,
                    pending: Coin::new(input.coin_id(), ctx.tree_hash(puzzle).into(), 0),
                    new_policy: definition(new_policy),
                    timelock_multiple: state.timelock_multiple,
                    initiation_seconds: execution.relative_seconds[0],
                    completion_seconds: policy.info().rekey_clawback,
                })
            }
        }
    }
}

pub(super) fn reconstruct<'a>(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    request: R<'a>,
    decoded: &[(NodePtr, NodePtr)],
) -> Result<Reconstruction<'a>, DriverError> {
    let first = &spends[0];
    match request {
        R::Payment { signers } => Ok(Reconstruction::Payment(reconstruct_payment(
            ctx,
            policy,
            spends,
            signers,
            decoded[0].1,
        )?)),
        R::PaymentClawback {
            destination,
            signers,
        } => {
            let fee_announcement = match_fee_form(
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
            Ok(Reconstruction::ready(E::PaymentClawback {
                destination,
                fee_announcement,
                pending: first.coin,
                refund,
                launcher_id: policy.info().launcher_id,
            }))
        }
        R::PaymentCompletion { destination } => {
            require_exact_spends(
                &[policy.completion_spend(ctx, first.coin, destination)?],
                spends,
            )?;
            Ok(Reconstruction::ready(E::PaymentCompletion {
                pending: first.coin,
                payout: Coin::new(first.coin.coin_id(), destination, first.coin.amount),
            }))
        }
        R::Rekey {
            new_policy,
            signers,
        } => reconstruct_rekey(ctx, policy, spends, new_policy, signers, decoded[0].1),
        R::RekeyClawback { state, signers } => {
            let fee_announcement = match_fee_form(
                |fee| {
                    policy
                        .rekey_clawback_spend(ctx, first.coin, state, signers, fee)
                        .map(|s| vec![s])
                },
                spends,
            )?;
            Ok(Reconstruction::ready(E::RekeyClawback {
                fee_announcement,
                pending: first.coin,
                new_root: state.new_root,
                timelock_multiple: state.timelock_multiple,
            }))
        }
        R::RekeyCompletion { new_policy } => reconstruct_completion(
            ctx,
            policy,
            spends,
            new_policy,
            decoded[0].1,
            decoded.get(1).map(|d| d.1),
        ),
        R::IncreaseSecurity {
            new_policy,
            signers,
        } => reconstruct_lock(ctx, policy, spends, new_policy, signers, decoded[0].1),
    }
}

fn reconstruct_rekey<'a>(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    new_policy: &'a InternalCustodyPolicy,
    signers: &[PublicKey],
    solution: NodePtr,
) -> Result<Reconstruction<'a>, DriverError> {
    let first = &spends[0];

    new_policy.verify_definition(ctx)?;
    let raw = singleton_solution(ctx, solution)?;
    let action = InternalCustodyRekeyArguments::from_clvm(&**ctx, raw.inner_solution.action)?;
    if action.new_root != new_policy.info().puzzle_root {
        return Err(InternalCustodyError::InvalidPolicy.into());
    }
    let state = InternalCustodyRekeyState {
        new_root: action.new_root,
        old_root: policy.info().puzzle_root,
        timelock_multiple: action.timelock_multiple,
    };
    let fee_announcement = match_fee_form(
        |fee| {
            policy
                .start_rekey_spend(
                    ctx,
                    first.coin,
                    raw.lineage_proof,
                    new_policy,
                    signers,
                    crate::InternalCustodyRekeyOptions {
                        timelock_multiple: state.timelock_multiple,
                        fee_announcement: fee,
                    },
                )
                .map(|s| vec![s])
        },
        spends,
    )?;
    Ok(Reconstruction::Rekey {
        state,
        proof: raw.lineage_proof,
        new_policy,
        fee_announcement,
    })
}

fn reconstruct_completion<'a>(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    new_policy: &InternalCustodyPolicy,
    solution: NodePtr,
    pending_solution: Option<NodePtr>,
) -> Result<Reconstruction<'a>, DriverError> {
    let first = &spends[0];
    let pending_solution = pending_solution.ok_or(InternalCustodyError::UnsupportedOperation)?;

    validate_target(ctx, policy, new_policy)?;
    if spends.len() != 2 {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    }
    let raw = singleton_solution(ctx, solution)?;
    let action = InternalCustodyRekeyArguments::from_clvm(&**ctx, raw.inner_solution.action)?;
    if action.new_root != new_policy.info().puzzle_root {
        return Err(InternalCustodyError::InvalidPolicy.into());
    }
    let node = pending_solution;
    let pending = InternalCustodyMerkleSolution::<NodePtr, NodePtr>::from_clvm(&**ctx, node)?;
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
    require_exact_spends(&expected, spends)?;
    Ok(Reconstruction::ready(E::RekeyCompletion {
        input: first.coin,
        pending: spends[1].coin,
        vault_child: crate::Singleton::new(first.coin, raw.lineage_proof, policy)
            .child_with(new_policy, first.coin.amount)
            .coin,
        new_policy: definition(new_policy),
    }))
}

fn reconstruct_lock<'a>(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    new_policy: &InternalCustodyPolicy,
    signers: &[PublicKey],
    solution: NodePtr,
) -> Result<Reconstruction<'a>, DriverError> {
    let first = &spends[0];

    validate_target(ctx, policy, new_policy)?;
    if policy.next_root() != Some(new_policy.info().puzzle_root) {
        return Err(InternalCustodyError::InvalidPolicy.into());
    }
    let raw = singleton_solution(ctx, solution)?;
    let stages = policy.increase_security_spends(ctx, first.coin, raw.lineage_proof, signers)?;
    let pending = stages.completion[0].coin;
    let intermediate_singleton = stages.completion[1].coin;
    let mut expected = vec![stages.authorization];
    if spends.len() != 1 {
        expected.extend(stages.completion);
    }
    require_exact_spends(&expected, spends)?;
    Ok(Reconstruction::ready(E::IncreaseSecurity {
        input: first.coin,
        intermediate_singleton,
        pending,
        vault_child: crate::Singleton::new(intermediate_singleton, raw.lineage_proof, policy)
            .child_with(new_policy, first.coin.amount)
            .coin,
        new_policy: definition(new_policy),
    }))
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
    solution: NodePtr,
) -> Result<
    SingletonSolution<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>>,
    DriverError,
> {
    Ok(FromClvm::from_clvm(&**ctx, solution)?)
}
pub(super) fn match_fee_form(
    mut build: impl FnMut(bool) -> Result<Vec<CoinSpend>, DriverError>,
    spends: &[CoinSpend],
) -> Result<bool, DriverError> {
    for fee in [false, true] {
        let expected = build(fee)?;
        if expected == spends {
            return Ok(fee);
        }
    }
    Err(InternalCustodyError::UnexpectedConditions.into())
}

fn require_exact_spends(expected: &[CoinSpend], actual: &[CoinSpend]) -> Result<(), DriverError> {
    if expected != actual {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    Ok(())
}
