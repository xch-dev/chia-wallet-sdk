use chia_protocol::Bytes32;
use clvm_utils::ToTreeHash;

use crate::ClawbackV2;

/// A payment that is paid out when a bag is unrolled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BagPayment {
    /// The puzzle hash of the recipient. For a clawback payment, this is the receiver puzzle hash.
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    /// If set, the payment is created as a clawback coin instead of being sent to the recipient directly.
    /// Its receiver puzzle hash and amount must match the payment.
    pub clawback: Option<ClawbackV2>,
}

impl BagPayment {
    pub fn new(puzzle_hash: Bytes32, amount: u64) -> Self {
        Self {
            puzzle_hash,
            amount,
            clawback: None,
        }
    }

    pub fn from_clawback(clawback: ClawbackV2) -> Self {
        Self {
            puzzle_hash: clawback.receiver_puzzle_hash,
            amount: clawback.amount,
            clawback: Some(clawback),
        }
    }

    /// The puzzle hash of the coin that the bag creates for this payment.
    pub fn coin_puzzle_hash(&self) -> Bytes32 {
        match self.clawback {
            Some(clawback) => clawback.tree_hash().into(),
            None => self.puzzle_hash,
        }
    }
}
