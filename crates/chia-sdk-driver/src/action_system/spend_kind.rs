use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_sdk_types::{
    conditions::{AssertPuzzleAnnouncement, CreateCoin},
    payment_assertion, tree_hash_notarized_payment, Conditions,
};
use clvmr::{Allocator, NodePtr};

use crate::{Output, OutputConstraints, OutputSet};

mod conditions_spend;
mod settlement_spend;

pub use conditions_spend::*;
pub use settlement_spend::*;

#[derive(Debug, Clone)]
pub enum SpendKind {
    Conditions(ConditionsSpend),
    Settlement(SettlementSpend),
}

impl SpendKind {
    pub fn conditions() -> Self {
        Self::Conditions(ConditionsSpend::new())
    }

    pub fn settlement() -> Self {
        Self::Settlement(SettlementSpend::new())
    }

    pub fn is_conditions(&self) -> bool {
        matches!(self, Self::Conditions(_))
    }

    pub fn is_settlement(&self) -> bool {
        matches!(self, Self::Settlement(_))
    }

    pub fn create_coin_with_assertion(
        &mut self,
        allocator: &Allocator,
        parent_puzzle_hash: Bytes32,
        payment_assertions: &mut Vec<AssertPuzzleAnnouncement>,
        create_coin: CreateCoin<NodePtr>,
    ) {
        match self {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().with(create_coin));
            }
            SpendKind::Settlement(spend) => {
                // TODO: Use nil for the nonce from the payment
                let notarized_payment = NotarizedPayment::new(
                    Bytes32::default(),
                    vec![Payment::new(
                        create_coin.puzzle_hash,
                        create_coin.amount,
                        create_coin.memos,
                    )],
                );
                payment_assertions.push(payment_assertion(
                    parent_puzzle_hash,
                    tree_hash_notarized_payment(allocator, &notarized_payment),
                ));
                spend.add_notarized_payment(notarized_payment);
            }
        }
    }

    pub fn create_intermediate_coin(&mut self, create_coin: CreateCoin<NodePtr>) {
        match self {
            Self::Conditions(spend) => {
                spend.add_conditions(Conditions::new().with(create_coin));
            }
            Self::Settlement(spend) => {
                // TODO: Use nil for the nonce from the payment
                spend.add_notarized_payment(NotarizedPayment {
                    nonce: Bytes32::default(),
                    payments: vec![Payment::new(
                        create_coin.puzzle_hash,
                        create_coin.amount,
                        create_coin.memos,
                    )],
                });
            }
        }
    }

    #[must_use]
    pub fn empty_copy(&self) -> Self {
        match self {
            Self::Conditions(_) => Self::conditions(),
            Self::Settlement(_) => Self::settlement(),
        }
    }
}

impl OutputSet for SpendKind {
    fn has_output(&self, output: &Output) -> bool {
        match self {
            Self::Conditions(spend) => spend.has_output(output),
            Self::Settlement(spend) => spend.has_output(output),
        }
    }

    fn can_run_cat_tail(&self) -> bool {
        match self {
            Self::Conditions(spend) => spend.can_run_cat_tail(),
            Self::Settlement(spend) => spend.can_run_cat_tail(),
        }
    }

    fn missing_singleton_output(&self) -> bool {
        match self {
            Self::Conditions(spend) => spend.missing_singleton_output(),
            Self::Settlement(spend) => spend.missing_singleton_output(),
        }
    }

    fn is_allowed(&self, output: &Output, output_constraints: &OutputConstraints) -> bool {
        match self {
            Self::Conditions(spend) => spend.is_allowed(output, output_constraints),
            Self::Settlement(spend) => spend.is_allowed(output, output_constraints),
        }
    }
}
