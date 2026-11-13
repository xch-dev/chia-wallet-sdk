use super::match_fee_form;
use crate::{
    DriverError, InternalCustodyError, InternalCustodyPayment, InternalCustodyPendingPayment,
    InternalCustodyPolicy, InternalCustodySingletonSolution, SpendContext,
};
use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_puzzle_types::Proof;
use chia_sdk_types::puzzles::InternalCustodyPaymentArguments;
use clvm_traits::FromClvm;
use clvmr::NodePtr;

pub(in crate::internal_custody) struct ReconstructedPayment {
    action: InternalCustodyPaymentArguments,
    proof: Proof,
    fee_announcement: bool,
}
impl ReconstructedPayment {
    pub(super) fn describe(
        self,
        ctx: &mut SpendContext,
        policy: &InternalCustodyPolicy,
        spends: &[CoinSpend],
        withdrawal_seconds: u64,
    ) -> Result<InternalCustodyPayment, DriverError> {
        let action = self.action;
        let fee_announcement = self.fee_announcement;
        let spend = &spends[0];
        let info = policy.info();
        let pending_puzzle = policy.pending_payment_puzzle(ctx, action.destination)?;
        let payment = InternalCustodyPayment {
            input_coin: spend.coin,
            vault_child: crate::Singleton::new(spend.coin, self.proof, policy)
                .child_with(
                    policy,
                    crate::internal_custody_remaining_amount(
                        spend.coin.amount,
                        action.incoming,
                        action.outgoing,
                    )?,
                )
                .coin,
            signed_destination_context: action.destination,
            incoming_amount: action.incoming,
            fee_announcement,
            deposit_count: u32::try_from(spends.len() - 1)
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
            withdrawal_seconds,
        };
        Ok(payment)
    }
}

pub(super) fn reconstruct_payment(
    ctx: &mut SpendContext,
    policy: &InternalCustodyPolicy,
    spends: &[CoinSpend],
    signers: &[PublicKey],
    solution: NodePtr,
) -> Result<ReconstructedPayment, DriverError> {
    // The operation verifier has already bounded and decoded this nonempty set.
    let spend = &spends[0];
    let singleton = InternalCustodySingletonSolution::from_clvm(&**ctx, solution)?;
    if singleton.inner_solution.spend_type
        != chia_sdk_types::puzzles::InternalCustodySpendType::HandlePayment
    {
        return Err(InternalCustodyError::UnsupportedOperation.into());
    }
    let action =
        InternalCustodyPaymentArguments::from_clvm(&**ctx, singleton.inner_solution.action)?;
    let deposits: Vec<_> = spends[1..].iter().map(|s| s.coin).collect();
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
    let fee_announcement = match_fee_form(
        |fee_announcement| {
            policy.payment_spends(
                ctx,
                spend.coin,
                singleton.lineage_proof,
                crate::InternalCustodyPaymentOptions {
                    action: payment_action,
                    fee_announcement,
                },
                signers,
                &deposits,
            )
        },
        spends,
    )?;
    Ok(ReconstructedPayment {
        action,
        proof: singleton.lineage_proof,
        fee_announcement,
    })
}
