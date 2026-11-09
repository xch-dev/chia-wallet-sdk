use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_sdk_types::Condition;
use clvmr::Allocator;

use crate::{
    AssertedPayment, DriverError, Facts, Reveals, TransferType, VerifiedSpend,
    parse_asserted_requested_payments, split_asserted_payments,
};

/// A future offer that the transaction sets coins aside for, as described by the fixed conditions of its
/// [`OfferPreSplit`](crate::TransferType::OfferPreSplit) children.
///
/// Anyone can spend the pre-split coins with their fixed conditions, so these are the terms the offer can be taken
/// on. Only payments asserted by every pre-split coin are included, so they must be made no matter which of the
/// pre-split coins are spent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedOffer {
    /// The total fee reserved by the fixed conditions of the pre-split coins.
    pub reserved_fee: u64,
    /// Revealed requested payments asserted by every pre-split coin, to one of the vault's known p2 puzzle hashes.
    pub received_payments: Vec<AssertedPayment>,
    /// Revealed requested payments asserted by every pre-split coin, to other puzzle hashes.
    pub external_payments: Vec<AssertedPayment>,
}

/// Rolls the [`OfferPreSplit`](crate::TransferType::OfferPreSplit) children of the spends up into a
/// [`LinkedOffer`], or returns `None` if there are none.
///
/// Returns [`DriverError::WrongLinkedOfferLauncherId`] if a pre-split coin belongs to a vault other than
/// `expected_launcher_id`.
pub fn build_linked_offer(
    reveals: &Reveals,
    allocator: &Allocator,
    spends: &[VerifiedSpend],
    expected_launcher_id: Bytes32,
    p2_puzzle_hashes: &HashSet<Bytes32>,
) -> Result<Option<LinkedOffer>, DriverError> {
    let mut linked_offer = LinkedOffer {
        reserved_fee: 0,
        received_payments: vec![],
        external_payments: vec![],
    };
    let mut has_offer = false;
    let mut reserved_fee = 0_u128;
    let mut found_puzzle_assertions: Option<HashSet<Bytes32>> = None;

    for spend in spends {
        for child in &spend.children {
            let TransferType::OfferPreSplit(info) = &child.transfer_type else {
                continue;
            };

            has_offer = true;

            if expected_launcher_id != info.launcher_id {
                return Err(DriverError::WrongLinkedOfferLauncherId);
            }

            let mut puzzle_assertions = HashSet::new();

            for condition in &info.fixed_conditions {
                match condition {
                    Condition::ReserveFee(condition) => {
                        reserved_fee += u128::from(condition.amount);
                    }
                    Condition::AssertPuzzleAnnouncement(condition) => {
                        puzzle_assertions.insert(condition.announcement_id);
                    }
                    _ => {}
                }
            }

            if let Some(found_puzzle_assertions) = &mut found_puzzle_assertions {
                *found_puzzle_assertions = found_puzzle_assertions
                    .intersection(&puzzle_assertions)
                    .copied()
                    .collect();
            } else {
                found_puzzle_assertions = Some(puzzle_assertions);
            }
        }
    }

    // Reserved fees that don't fit in a u64 can never be satisfied on chain.
    linked_offer.reserved_fee = reserved_fee.try_into()?;

    if let Some(found_puzzle_assertions) = found_puzzle_assertions {
        let mut offer_facts = Facts::default();

        for announcement_id in found_puzzle_assertions {
            offer_facts.assert_puzzle_announcement(announcement_id);
        }

        let asserted_payments =
            parse_asserted_requested_payments(reveals, &offer_facts, allocator)?;
        let split_payments =
            split_asserted_payments(&asserted_payments, p2_puzzle_hashes, reveals.asset_info());
        linked_offer.received_payments = split_payments.received_payments;
        linked_offer.external_payments = split_payments.external_payments;
    }

    Ok(has_offer.then_some(linked_offer))
}
