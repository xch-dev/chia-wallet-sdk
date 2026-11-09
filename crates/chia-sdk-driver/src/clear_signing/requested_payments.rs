use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{announcement_id, tree_hash_notarized_payment};
use clvmr::Allocator;

use crate::{AssetInfo, CatInfo, DriverError, Facts, HashedPtr, NftInfo, Reveals, SingletonInfo};

/// A revealed notarized payment of a settlement coin whose announcement is asserted, so the transaction is only
/// valid if some settlement coin of the asset makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedNotarizedPayment {
    /// The asset being paid.
    pub asset: ClearSigningAsset,
    /// The nonce and payments.
    pub notarized_payment: NotarizedPayment,
}

/// A single payment of an [`AssertedNotarizedPayment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedPayment {
    /// The asset being paid.
    pub asset: ClearSigningAsset,
    /// The nonce of the notarized payment. For NFT royalties, this is the NFT's launcher id.
    pub nonce: Bytes32,
    /// The puzzle hash, amount, and memos of the payment.
    pub payment: Payment,
    /// The royalty of the NFT if the nonce is the launcher id of a revealed NFT and the payment is to its royalty
    /// puzzle hash, which suggests it's a royalty payment.
    pub royalty_basis_points: Option<u16>,
}

/// Asserted payments, split by whether they're paid to the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAssertedPayments {
    /// Payments to one of the vault's known p2 puzzle hashes.
    pub received_payments: Vec<AssertedPayment>,
    /// Payments to any other puzzle hash.
    pub external_payments: Vec<AssertedPayment>,
}

/// An asset, as identified for clear signing. Unlike an asset id alone, this includes every detail that affects
/// the asset's puzzle hash, so two assets with the same id but different details are different.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearSigningAsset {
    /// XCH.
    Xch,
    /// A CAT.
    Cat {
        /// The asset id, which is the tree hash of the TAIL.
        asset_id: Bytes32,
        /// The hidden puzzle hash of the revocation layer, or `None` if the CAT isn't revocable. A revocable CAT
        /// can be taken back by its issuer.
        hidden_puzzle_hash: Option<Bytes32>,
    },
    /// An NFT.
    Nft {
        /// The launcher id.
        launcher_id: Bytes32,
        /// The metadata.
        metadata: HashedPtr,
        /// The puzzle hash of the metadata updater.
        metadata_updater_puzzle_hash: Bytes32,
        /// The puzzle hash that royalties are paid to.
        royalty_puzzle_hash: Bytes32,
        /// The royalty percentage, where 100 basis points is 1%.
        royalty_basis_points: u16,
    },
}

/// Splits each payment of the asserted notarized payments into received payments (to one of the `p2_puzzle_hashes`)
/// and external payments (to anything else).
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

/// The revealed requested payments whose puzzle announcements are asserted in the [`Facts`], deduplicated by
/// announcement id.
///
/// Requested payments are revealed by settlement spends in the [`Reveals`]. A payment is only included if the
/// announcement of a settlement coin of its asset (XCH, or a CAT or NFT whose details were revealed) is asserted,
/// which means the transaction requires the payment to be made. Payments of option contracts aren't included, and NFT
/// payments are only matched for an NFT without an owner.
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
