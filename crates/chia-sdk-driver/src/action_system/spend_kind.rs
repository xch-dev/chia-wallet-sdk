use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_sdk_types::{
    Conditions,
    conditions::{AssertPuzzleAnnouncement, CreateCoin},
    payment_assertion, tree_hash_notarized_payment,
};
use clvmr::{Allocator, NodePtr};

use crate::{Output, OutputConstraints, OutputSet};

mod conditions_spend;
mod settlement_spend;

pub use conditions_spend::*;
pub use settlement_spend::*;

/// What a coin's p2 puzzle will output when it's spent, as built up by the actions.
#[derive(Debug, Clone)]
pub enum SpendKind {
    /// The p2 puzzle outputs arbitrary conditions, such as the standard puzzle.
    Conditions(ConditionsSpend),
    /// The p2 puzzle is the settlement payments puzzle, which can only make notarized payments.
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

    /// Creates a coin from the parent and, if the parent is a settlement coin, records an
    /// assertion that the payment was made.
    ///
    /// Settlement payments are notarized with the parent coin id as the nonce. Settlement coins
    /// can be spent by anyone, so if two settlement coins with the same puzzle hash made the same
    /// payment with the same nonce, their announcements would be identical. A single assertion
    /// would then be satisfied by either coin, and a third party could remove one of the spends
    /// from the bundle and claim the coin for themselves.
    pub fn create_coin_with_assertion(
        &mut self,
        allocator: &Allocator,
        parent_coin: Coin,
        payment_assertions: &mut Vec<AssertPuzzleAnnouncement>,
        create_coin: CreateCoin<NodePtr>,
    ) {
        match self {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().with(create_coin));
            }
            SpendKind::Settlement(spend) => {
                let notarized_payment = NotarizedPayment::new(
                    parent_coin.coin_id(),
                    vec![Payment::new(
                        create_coin.puzzle_hash,
                        create_coin.amount,
                        create_coin.memos,
                    )],
                );
                payment_assertions.push(payment_assertion(
                    parent_coin.puzzle_hash,
                    tree_hash_notarized_payment(allocator, &notarized_payment),
                ));
                spend.add_notarized_payment(notarized_payment);
            }
        }
    }

    /// Creates an ephemeral coin from the parent, which will be spent in the same transaction.
    ///
    /// Unlike [`SpendKind::create_coin_with_assertion`], no payment assertion is needed, since
    /// the child spend can't exist without the parent spend.
    pub fn create_intermediate_coin(
        &mut self,
        parent_coin_id: Bytes32,
        create_coin: CreateCoin<NodePtr>,
    ) {
        match self {
            Self::Conditions(spend) => {
                spend.add_conditions(Conditions::new().with(create_coin));
            }
            Self::Settlement(spend) => {
                spend.add_notarized_payment(NotarizedPayment::new(
                    parent_coin_id,
                    vec![Payment::new(
                        create_coin.puzzle_hash,
                        create_coin.amount,
                        create_coin.memos,
                    )],
                ));
            }
        }
    }

    /// A spend of the same kind with nothing in it yet, for the child of a singleton.
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
