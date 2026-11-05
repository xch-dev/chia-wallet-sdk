use std::convert::Infallible;

use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_types::{
    Condition, Nil,
    conditions::{CreateCoin, CreateCoinAnnouncement},
};
use clvm_traits::{clvm_list, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, Spend, SpendContext};

use super::BagChild;

/// The message of the coin announcement that each node makes, so that a fee spend can assert it.
const FEE_ANNOUNCEMENT: &[u8] = b"$";

/// The default leaf width for a number of payments.
pub(super) fn default_leaf_width(payment_count: usize) -> usize {
    match payment_count {
        0..=1000 => 100,
        1001..=10_000 => 250,
        10_001..=100_000 => 500,
        _ => 1000,
    }
}

/// Sorts by the puzzle hash of the coin each payment creates, so clawback payments are keyed by their clawback puzzle hash.
pub(super) fn sort(children: &mut [BagChild]) {
    children.sort_by_key(BagChild::puzzle_hash);
}

/// A single payment without a clawback isn't bagged, but a clawback still needs a node to create it.
pub(super) fn is_root(level: &[BagChild]) -> bool {
    matches!(level, [child] if !child.is_clawback())
}

pub(super) fn node_puzzle_hash(children: &[BagChild]) -> Bytes32 {
    let Ok(conditions) = node_conditions(children, |memo| {
        Ok::<TreeHash, Infallible>(match memo {
            Memo::Hint(hint) => clvm_list!(hint).tree_hash(),
            Memo::Clawback(receiver, memo) => clvm_list!(receiver, memo).tree_hash(),
        })
    });
    clvm_quote!(conditions).tree_hash().into()
}

pub(super) fn node_spend(
    ctx: &mut SpendContext,
    children: &[BagChild],
) -> Result<Spend, DriverError> {
    let conditions = node_conditions(children, |memo| match memo {
        Memo::Hint(hint) => ctx.alloc(&clvm_list!(hint)),
        Memo::Clawback(receiver, memo) => ctx.alloc(&clvm_list!(receiver, memo)),
    })?;
    let puzzle = ctx.alloc(&clvm_quote!(conditions))?;

    Ok(Spend::new(puzzle, NodePtr::NIL))
}

enum Memo {
    Hint(Bytes32),
    Clawback(Bytes32, (Bytes32, (u64, ()))),
}

/// Payments are hinted with their puzzle hash, and clawback payments with the receiver and the clawback memo,
/// so that the clawback can be parsed from the parent spend.
fn child_memo(child: &BagChild) -> Memo {
    match child {
        BagChild::Node { puzzle_hash, .. } => Memo::Hint(*puzzle_hash),
        BagChild::Payment { payment, .. } => match payment.clawback {
            Some(clawback) => Memo::Clawback(clawback.receiver_puzzle_hash, clawback.memo()),
            None => Memo::Hint(payment.puzzle_hash),
        },
    }
}

fn node_conditions<T: Nil, E>(
    children: &[BagChild],
    mut memos: impl FnMut(Memo) -> Result<T, E>,
) -> Result<Vec<Condition<T>>, E> {
    let mut conditions = Vec::with_capacity(children.len() + 1);

    conditions.push(Condition::CreateCoinAnnouncement(
        CreateCoinAnnouncement::new(Bytes::new(FEE_ANNOUNCEMENT.to_vec())),
    ));

    for child in children {
        conditions.push(Condition::CreateCoin(CreateCoin::new(
            child.puzzle_hash(),
            child.amount(),
            Memos::Some(memos(child_memo(child))?),
        )));
    }

    Ok(conditions)
}
