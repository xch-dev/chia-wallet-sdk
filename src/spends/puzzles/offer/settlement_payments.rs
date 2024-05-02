use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{FromClvm, ToClvm};

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
