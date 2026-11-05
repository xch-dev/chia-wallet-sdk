use chia_protocol::Bytes32;
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{Nil, announcement_id, conditions::AssertPuzzleAnnouncement};

pub fn payment_assertion<R: Nil>(
    puzzle_hash: Bytes32,
    notarized_payment_hash: TreeHash,
) -> AssertPuzzleAnnouncement<R> {
    AssertPuzzleAnnouncement::new(announcement_id(puzzle_hash, notarized_payment_hash))
}

pub fn tree_hash_notarized_payment(
    allocator: &Allocator,
    notarized_payment: &NotarizedPayment<NodePtr>,
) -> TreeHash {
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
    .tree_hash()
}
