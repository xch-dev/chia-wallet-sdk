use chia_puzzle_types::offer::NotarizedPayment;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{announcement_id, tree_hash_notarized_payment};
use clvmr::Allocator;

use crate::{CatAssetInfo, CatInfo, DriverError, Facts, NftAssetInfo, NftInfo, SingletonInfo};

#[derive(Debug, Clone)]
pub struct AssertedRequestedPayment {
    pub asset: RequestedAsset,
    pub notarized_payment: NotarizedPayment,
}

#[derive(Debug, Clone, Copy)]
pub enum RequestedAsset {
    Xch,
    Cat(CatAssetInfo),
    Nft(NftAssetInfo),
}

pub fn parse_asserted_requested_payments(
    facts: &Facts,
    allocator: &Allocator,
) -> Result<Vec<AssertedRequestedPayment>, DriverError> {
    let mut payments = Vec::new();

    let requested_payments = facts.requested_payments();
    let asset_info = facts.asset_info();

    for notarized_payment in &requested_payments.xch {
        let hash = tree_hash_notarized_payment(allocator, notarized_payment);
        let announcement_id = announcement_id(SETTLEMENT_PAYMENT_HASH.into(), hash);

        if facts.is_puzzle_announcement_asserted(announcement_id) {
            payments.push(AssertedRequestedPayment {
                asset: RequestedAsset::Xch,
                notarized_payment: notarized_payment.clone(),
            });
        }
    }

    for (&asset_id, notarized_payments) in &requested_payments.cats {
        for notarized_payment in notarized_payments {
            let info = asset_info
                .cat(asset_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let hash = tree_hash_notarized_payment(allocator, notarized_payment);
            let puzzle_hash = CatInfo::new(
                asset_id,
                info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash();
            let announcement_id = announcement_id(puzzle_hash.into(), hash);

            if facts.is_puzzle_announcement_asserted(announcement_id) {
                payments.push(AssertedRequestedPayment {
                    asset: RequestedAsset::Cat(*info),
                    notarized_payment: notarized_payment.clone(),
                });
            }
        }
    }

    for (&launcher_id, notarized_payments) in &requested_payments.nfts {
        for notarized_payment in notarized_payments {
            let info = asset_info
                .nft(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

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

            if facts.is_puzzle_announcement_asserted(announcement_id) {
                payments.push(AssertedRequestedPayment {
                    asset: RequestedAsset::Nft(*info),
                    notarized_payment: notarized_payment.clone(),
                });
            }
        }
    }

    Ok(payments)
}
