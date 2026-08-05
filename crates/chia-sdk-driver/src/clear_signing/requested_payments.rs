use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{announcement_id, tree_hash_notarized_payment};
use clvmr::Allocator;

use crate::{AssetInfo, CatInfo, DriverError, Facts, HashedPtr, NftInfo, Reveals, SingletonInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedNotarizedPayment {
    pub asset: ClearSigningAsset,
    pub notarized_payment: NotarizedPayment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedPayment {
    pub asset: ClearSigningAsset,
    pub nonce: Bytes32,
    pub payment: Payment,
    pub royalty_basis_points: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAssertedPayments {
    pub received_payments: Vec<AssertedPayment>,
    pub external_payments: Vec<AssertedPayment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearSigningAsset {
    Xch,
    Cat {
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
    },
    Nft {
        launcher_id: Bytes32,
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    },
}

pub fn split_asserted_payments(
    asserted_payments: &[AssertedNotarizedPayment],
    p2_puzzle_hashes: &HashSet<Bytes32>,
    asset_info: &AssetInfo,
) -> SplitAssertedPayments {
    let mut received_payments = Vec::new();
    let mut external_payments = Vec::new();

    for asserted_notarized_payment in asserted_payments {
        for payment in &asserted_notarized_payment.notarized_payment.payments {
            let royalty_basis_points = royalty_basis_points(
                asset_info,
                asserted_notarized_payment.notarized_payment.nonce,
                payment,
            );

            let asserted_payment = AssertedPayment {
                asset: asserted_notarized_payment.asset,
                nonce: asserted_notarized_payment.notarized_payment.nonce,
                payment: payment.clone(),
                royalty_basis_points,
            };

            if p2_puzzle_hashes.contains(&asserted_payment.payment.puzzle_hash) {
                received_payments.push(asserted_payment);
            } else {
                external_payments.push(asserted_payment);
            }
        }
    }

    SplitAssertedPayments {
        received_payments,
        external_payments,
    }
}

fn royalty_basis_points(asset_info: &AssetInfo, nonce: Bytes32, payment: &Payment) -> Option<u16> {
    let nft = asset_info.nft(nonce)?;

    (payment.puzzle_hash == nft.royalty_puzzle_hash).then_some(nft.royalty_basis_points)
}

pub fn parse_asserted_requested_payments(
    reveals: &Reveals,
    facts: &Facts,
    allocator: &Allocator,
) -> Result<Vec<AssertedNotarizedPayment>, DriverError> {
    let mut payments = Vec::new();
    let mut seen_announcements = HashSet::new();

    let requested_payments = reveals.requested_payments();
    let asset_info = reveals.asset_info();

    for notarized_payment in &requested_payments.xch {
        let hash = tree_hash_notarized_payment(allocator, notarized_payment);
        let announcement_id = announcement_id(SETTLEMENT_PAYMENT_HASH.into(), hash);

        let payment = AssertedNotarizedPayment {
            asset: ClearSigningAsset::Xch,
            notarized_payment: notarized_payment.clone(),
        };

        if facts.is_puzzle_announcement_asserted(announcement_id)
            && seen_announcements.insert(announcement_id)
        {
            payments.push(payment);
        }
    }

    for (&asset_id, notarized_payments) in &requested_payments.cats {
        for notarized_payment in notarized_payments {
            let Some(info) = asset_info.cat(asset_id) else {
                continue;
            };

            let hash = tree_hash_notarized_payment(allocator, notarized_payment);
            let puzzle_hash = CatInfo::new(
                asset_id,
                info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash();
            let announcement_id = announcement_id(puzzle_hash.into(), hash);

            let payment = AssertedNotarizedPayment {
                asset: ClearSigningAsset::Cat {
                    asset_id,
                    hidden_puzzle_hash: info.hidden_puzzle_hash,
                },
                notarized_payment: notarized_payment.clone(),
            };

            if facts.is_puzzle_announcement_asserted(announcement_id)
                && seen_announcements.insert(announcement_id)
            {
                payments.push(payment);
            }
        }
    }

    for (&launcher_id, notarized_payments) in &requested_payments.nfts {
        for notarized_payment in notarized_payments {
            let Some(info) = asset_info.nft(launcher_id) else {
                continue;
            };

            let hash = tree_hash_notarized_payment(allocator, notarized_payment);
            let puzzle_hash = NftInfo::new(
                launcher_id,
                info.metadata,
                info.metadata_updater_puzzle_hash,
                None,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash();
            let announcement_id = announcement_id(puzzle_hash.into(), hash);

            let payment = AssertedNotarizedPayment {
                asset: ClearSigningAsset::Nft {
                    launcher_id,
                    metadata: info.metadata,
                    metadata_updater_puzzle_hash: info.metadata_updater_puzzle_hash,
                    royalty_puzzle_hash: info.royalty_puzzle_hash,
                    royalty_basis_points: info.royalty_basis_points,
                },
                notarized_payment: notarized_payment.clone(),
            };

            if facts.is_puzzle_announcement_asserted(announcement_id)
                && seen_announcements.insert(announcement_id)
            {
                payments.push(payment);
            }
        }
    }

    Ok(payments)
}
