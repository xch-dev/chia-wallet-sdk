use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_sdk_types::Condition;
use clvmr::Allocator;

use crate::{
    AssertedPayment, DriverError, Facts, Reveals, TransferType, VerifiedSpend,
    parse_asserted_requested_payments, split_asserted_payments,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedOffer {
    pub reserved_fee: u64,
    pub received_payments: Vec<AssertedPayment>,
    pub external_payments: Vec<AssertedPayment>,
}

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

            let mut reserved_fee = 0;
            let mut puzzle_assertions = HashSet::new();

            for condition in &info.fixed_conditions {
                match condition {
                    Condition::ReserveFee(condition) => {
                        reserved_fee += condition.amount;
                    }
                    Condition::AssertPuzzleAnnouncement(condition) => {
                        puzzle_assertions.insert(condition.announcement_id);
                    }
                    _ => {}
                }
            }

            linked_offer.reserved_fee += reserved_fee;

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
