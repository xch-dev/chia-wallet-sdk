use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{SpendContext, SpendError};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(tuple)]
pub struct SettlementPaymentsSolution {
    pub notarized_payments: Vec<NotarizedPayment>,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(tuple)]
pub struct NotarizedPayment {
    pub nonce: Bytes32,
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(tuple, untagged)]
pub enum Payment {
    WithoutMemos(PaymentWithoutMemos),
    WithMemos(PaymentWithMemos),
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PaymentWithoutMemos {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PaymentWithMemos {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Vec<Bytes>,
}

pub fn calculate_nonce(
    ctx: &mut SpendContext,
    mut offered_coin_ids: Vec<Bytes32>,
) -> Result<Bytes32, SpendError> {
    offered_coin_ids.sort();
    let offered_coin_ids = ctx.alloc(offered_coin_ids)?;
    Ok(ctx.tree_hash(offered_coin_ids))
}

pub struct OfferRequest {
    pub coin_spend: CoinSpend,
    pub puzzle_announcement_id: Bytes32,
}

pub fn request_offer_payments(
    ctx: &mut SpendContext,
    nonce: Bytes32,
    requested_puzzle: NodePtr,
    requested_payments: Vec<Payment>,
) -> Result<OfferRequest, SpendError> {
    let puzzle_reveal = ctx.serialize(requested_puzzle)?;
    let puzzle_hash = ctx.tree_hash(requested_puzzle);

    let notarized_payment = NotarizedPayment {
        nonce,
        payments: requested_payments,
    };

    let settlement_solution = ctx.serialize(SettlementPaymentsSolution {
        notarized_payments: vec![notarized_payment.clone()],
    })?;

    let coin_spend = CoinSpend::new(
        Coin::new(Bytes32::default(), puzzle_hash, 0),
        puzzle_reveal,
        settlement_solution,
    );

    let notarized_payment_ptr = ctx.alloc(notarized_payment)?;
    let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

    let mut hasher = Sha256::new();
    hasher.update(puzzle_hash);
    hasher.update(notarized_payment_hash);
    let puzzle_announcement_id = Bytes32::new(hasher.finalize_fixed().into());

    Ok(OfferRequest {
        coin_spend,
        puzzle_announcement_id,
    })
}
