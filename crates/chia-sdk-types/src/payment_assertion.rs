use chia_protocol::Bytes32;
use chia_puzzle_types::{
    offer::{NotarizedPayment, Payment},
    Memos,
};
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{announcement_id, conditions::AssertPuzzleAnnouncement};

pub fn payment_assertion(
    puzzle_hash: Bytes32,
    notarized_payment: &NotarizedPayment<TreeHash>,
) -> AssertPuzzleAnnouncement {
    AssertPuzzleAnnouncement::new(announcement_id(puzzle_hash, notarized_payment.tree_hash()))
}

pub fn tree_hash_notarized_payment(
    allocator: &Allocator,
    notarized_payment: &NotarizedPayment<NodePtr>,
) -> NotarizedPayment<TreeHash> {
    NotarizedPayment {
        nonce: notarized_payment.nonce,
        payments: notarized_payment
            .payments
            .iter()
            .map(|payment| Payment {
                puzzle_hash: payment.puzzle_hash,
                amount: payment.amount,
                memos: match payment.memos {
                    Memos::Some(memos) => Memos::Some(tree_hash(allocator, memos)),
                    Memos::None => Memos::None,
                },
            })
            .collect(),
    }
}
