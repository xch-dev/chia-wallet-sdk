use chia_protocol::Bytes32;
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{announcement_id, conditions::AssertPuzzleAnnouncement};

/// Creates the condition that asserts a notarized payment was made by a settlement payments coin.
///
/// The settlement payments puzzle creates a puzzle announcement of the tree hash of each
/// notarized payment it pays out. This is how one side of an offer makes sure that it only goes
/// through if the other side's requested payment is made. The `puzzle_hash` is the puzzle hash of
/// the settlement payments coin, which is [`SettlementPayment`](crate::puzzles::SettlementPayment)
/// for XCH, or that puzzle wrapped in an outer layer such as the CAT layer for other assets.
///
/// Use [`tree_hash_notarized_payment`] to compute the `notarized_payment_hash`.
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::Bytes32;
/// # use chia_puzzle_types::{Memos, offer::{NotarizedPayment, Payment}};
/// # use chia_sdk_types::{
/// #     Mod, announcement_id, payment_assertion, puzzles::SettlementPayment,
/// #     tree_hash_notarized_payment,
/// # };
/// # use clvmr::Allocator;
/// let allocator = Allocator::new();
///
/// let notarized_payment = NotarizedPayment::new(
///     Bytes32::new([1; 32]),
///     vec![Payment::new(Bytes32::new([2; 32]), 1000, Memos::None)],
/// );
/// let hash = tree_hash_notarized_payment(&allocator, &notarized_payment);
///
/// let settlement_puzzle_hash = SettlementPayment::mod_hash().into();
/// let assertion = payment_assertion(settlement_puzzle_hash, hash);
///
/// assert_eq!(
///     assertion.announcement_id,
///     announcement_id(settlement_puzzle_hash, hash)
/// );
/// ```
pub fn payment_assertion(
    puzzle_hash: Bytes32,
    notarized_payment_hash: TreeHash,
) -> AssertPuzzleAnnouncement {
    AssertPuzzleAnnouncement::new(announcement_id(puzzle_hash, notarized_payment_hash))
}

/// Computes the tree hash of a [`NotarizedPayment`] whose memos are stored in an [`Allocator`].
///
/// This is the message of the puzzle announcement that the settlement payments puzzle creates
/// for the payment. The memos of each payment are hashed with the allocator, so they don't need
/// to be converted first.
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
