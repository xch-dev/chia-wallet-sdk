use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, SettlementPaymentsSolution};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::AssertPuzzleAnnouncement, payment_assertion, tree_hash_notarized_payment,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::{
    Action, AssetInfo, CatAssetInfo, CatInfo, DriverError, HashedPtr, Id, Layer, NftAssetInfo,
    NftInfo, OfferAmounts, OptionAssetInfo, OptionInfo, Puzzle, SettlementLayer, SpendContext,
};

#[derive(Debug, Default, Clone)]
pub struct RequestedPayments {
    pub xch: Vec<NotarizedPayment>,
    pub cats: IndexMap<Bytes32, Vec<NotarizedPayment>>,
    pub nfts: IndexMap<Bytes32, Vec<NotarizedPayment>>,
    pub options: IndexMap<Bytes32, Vec<NotarizedPayment>>,
}

impl RequestedPayments {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn amounts(&self) -> OfferAmounts {
        OfferAmounts {
            xch: self
                .xch
                .iter()
                .flat_map(|np| np.payments.iter().map(|p| p.amount))
                .sum(),
            cats: self
                .cats
                .iter()
                .map(|(&launcher_id, nps)| {
                    (
                        launcher_id,
                        nps.iter()
                            .flat_map(|np| np.payments.iter().map(|p| p.amount))
                            .sum(),
                    )
                })
                .collect(),
        }
    }

    pub fn assertions(
        &self,
        ctx: &mut SpendContext,
        asset_info: &AssetInfo,
    ) -> Result<Vec<AssertPuzzleAnnouncement>, DriverError> {
        let mut assertions = Vec::new();

        for notarized_payment in &self.xch {
            assertions.push(payment_assertion(
                SETTLEMENT_PAYMENT_HASH.into(),
                tree_hash_notarized_payment(ctx, notarized_payment),
            ));
        }

        for (&asset_id, notarized_payments) in &self.cats {
            let default = CatAssetInfo::default();
            let info = asset_info.cat(asset_id).unwrap_or(&default);

            let puzzle_hash = CatInfo::new(
                asset_id,
                info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash()
            .into();

            for notarized_payment in notarized_payments {
                assertions.push(payment_assertion(
                    puzzle_hash,
                    tree_hash_notarized_payment(ctx, notarized_payment),
                ));
            }
        }

        for (&launcher_id, notarized_payments) in &self.nfts {
            let info = asset_info
                .nft(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let puzzle_hash = NftInfo::new(
                launcher_id,
                info.metadata,
                info.metadata_updater_puzzle_hash,
                None,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash()
            .into();

            for notarized_payment in notarized_payments {
                assertions.push(payment_assertion(
                    puzzle_hash,
                    tree_hash_notarized_payment(ctx, notarized_payment),
                ));
            }
        }

        for (&launcher_id, notarized_payments) in &self.options {
            let info = asset_info
                .option(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let puzzle_hash = OptionInfo::new(
                launcher_id,
                info.underlying_coin_id,
                info.underlying_delegated_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash()
            .into();

            for notarized_payment in notarized_payments {
                assertions.push(payment_assertion(
                    puzzle_hash,
                    tree_hash_notarized_payment(ctx, notarized_payment),
                ));
            }
        }

        Ok(assertions)
    }

    pub fn actions(&self) -> Vec<Action> {
        let mut actions = Vec::new();

        for notarized_payment in &self.xch {
            actions.push(Action::settle(Id::Xch, notarized_payment.clone()));
        }

        for (&asset_id, notarized_payments) in &self.cats {
            for notarized_payment in notarized_payments {
                actions.push(Action::settle(
                    Id::Existing(asset_id),
                    notarized_payment.clone(),
                ));
            }
        }

        for (&launcher_id, notarized_payments) in &self.nfts {
            for notarized_payment in notarized_payments {
                actions.push(Action::settle(
                    Id::Existing(launcher_id),
                    notarized_payment.clone(),
                ));
            }
        }

        for (&launcher_id, notarized_payments) in &self.options {
            for notarized_payment in notarized_payments {
                actions.push(Action::settle(
                    Id::Existing(launcher_id),
                    notarized_payment.clone(),
                ));
            }
        }

        actions
    }

    pub fn extend(&mut self, other: Self) -> Result<(), DriverError> {
        for payment in other.xch {
            self.xch.push(payment);
        }

        for (asset_id, payments) in other.cats {
            self.cats.entry(asset_id).or_default().extend(payments);
        }

        for (launcher_id, payments) in other.nfts {
            self.nfts.entry(launcher_id).or_default().extend(payments);
        }

        for (launcher_id, payments) in other.options {
            self.options
                .entry(launcher_id)
                .or_default()
                .extend(payments);
        }

        Ok(())
    }

    pub fn parse(
        &mut self,
        allocator: &Allocator,
        asset_info: &mut AssetInfo,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<(), DriverError> {
        let notarized_payments =
            SettlementPaymentsSolution::from_clvm(allocator, solution)?.notarized_payments;

        if SettlementLayer::parse_puzzle(allocator, puzzle)?.is_some() {
            self.xch.extend(notarized_payments);
        } else if let Some((cat, _)) = CatInfo::parse(allocator, puzzle)? {
            self.cats
                .entry(cat.asset_id)
                .or_default()
                .extend(notarized_payments);

            let info = CatAssetInfo::new(cat.hidden_puzzle_hash);
            asset_info.insert_cat(cat.asset_id, info)?;
        } else if let Some((nft, _)) = NftInfo::<HashedPtr>::parse(allocator, puzzle)? {
            self.nfts
                .entry(nft.launcher_id)
                .or_default()
                .extend(notarized_payments);

            let info = NftAssetInfo::new(
                nft.metadata,
                nft.metadata_updater_puzzle_hash,
                nft.royalty_puzzle_hash,
                nft.royalty_basis_points,
            );
            asset_info.insert_nft(nft.launcher_id, info)?;
        } else if let Some((option, _)) = OptionInfo::parse(allocator, puzzle)? {
            self.options
                .entry(option.launcher_id)
                .or_default()
                .extend(notarized_payments);

            let info = OptionAssetInfo::new(
                option.underlying_coin_id,
                option.underlying_delegated_puzzle_hash,
            );
            asset_info.insert_option(option.launcher_id, info)?;
        }

        Ok(())
    }
}
