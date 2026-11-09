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
    Action, AssetInfo, CatAssetInfo, CatInfo, DriverError, Id, Layer, NftAssetInfo, NftInfo,
    OfferAmounts, OptionAssetInfo, OptionInfo, Puzzle, SettlementLayer, SingletonInfo,
    SpendContext,
};

/// The payments that the maker of an offer requests in return.
///
/// Each payment is a [`NotarizedPayment`], which the taker makes by spending a settlement coin of
/// the asset. The maker's spends assert the puzzle announcements of these payments (see
/// [`RequestedPayments::assertions`]), so the offer can only be taken if they are made. The nonce
/// of a notarized payment is usually [`Offer::nonce`](crate::Offer::nonce) of the offered coins.
#[derive(Debug, Default, Clone)]
pub struct RequestedPayments {
    /// The requested XCH payments.
    pub xch: Vec<NotarizedPayment>,
    /// The requested CAT payments, keyed by asset id.
    pub cats: IndexMap<Bytes32, Vec<NotarizedPayment>>,
    /// The requested NFT payments, keyed by launcher id.
    pub nfts: IndexMap<Bytes32, Vec<NotarizedPayment>>,
    /// The requested option contract payments, keyed by launcher id.
    pub options: IndexMap<Bytes32, Vec<NotarizedPayment>>,
}

impl RequestedPayments {
    /// Creates an empty [`RequestedPayments`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The total requested amounts of XCH and each CAT.
    pub fn amounts(&self) -> OfferAmounts {
        fn total(nps: &[NotarizedPayment]) -> u128 {
            nps.iter()
                .flat_map(|np| np.payments.iter().map(|p| u128::from(p.amount)))
                .sum()
        }

        OfferAmounts {
            xch: total(&self.xch),
            cats: self
                .cats
                .iter()
                .map(|(&asset_id, nps)| (asset_id, total(nps)))
                .collect(),
        }
    }

    /// The payments of a CAT, or of XCH if `asset_id` is `None`.
    pub fn fungible_mut(&mut self, asset_id: Option<Bytes32>) -> &mut Vec<NotarizedPayment> {
        match asset_id {
            Some(asset_id) => self.cats.entry(asset_id).or_default(),
            None => &mut self.xch,
        }
    }

    /// Each XCH and CAT payment, along with its CAT asset id, which is `None` for XCH.
    pub fn fungible(&self) -> impl Iterator<Item = (Option<Bytes32>, &NotarizedPayment)> {
        let xch = self.xch.iter().map(|payment| (None, payment));

        let cats = self.cats.iter().flat_map(|(&asset_id, payments)| {
            payments
                .iter()
                .map(move |payment| (Some(asset_id), payment))
        });

        xch.chain(cats)
    }

    /// The `ASSERT_PUZZLE_ANNOUNCEMENT` conditions that the maker's spends must output, so that the
    /// offer can only be taken if the requested payments are made.
    ///
    /// Returns [`DriverError::MissingAssetInfo`] if a requested CAT, NFT, or option contract has no
    /// asset info, since the puzzle hash of its settlement coin can't be calculated.
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
            let puzzle_hash = asset_info.settlement_puzzle_hash(Some(asset_id))?;

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

    /// The settle actions that make the requested payments, for the taker to apply with
    /// [`Spends::apply`](crate::Spends::apply).
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

    /// Adds the payments from another [`RequestedPayments`], for example when combining offers.
    pub fn extend(&mut self, other: Self) {
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
    }

    /// Parses the notarized payments of a settlement coin spend from an offer, and adds them.
    ///
    /// The asset info of requested CATs, NFTs, and option contracts is added to the [`AssetInfo`].
    /// Returns [`DriverError::UnsupportedRequestedAsset`] if the puzzle isn't the settlement
    /// payments puzzle, or a CAT, NFT, or option contract with it as the p2 puzzle.
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
        } else if let Some((cat, _)) = CatInfo::parse(allocator, puzzle)?
            && cat.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
        {
            self.cats
                .entry(cat.asset_id)
                .or_default()
                .extend(notarized_payments);

            let info = CatAssetInfo::new(cat.hidden_puzzle_hash);
            asset_info.insert_cat(cat.asset_id, info)?;
        } else if let Some((nft, _)) = NftInfo::parse(allocator, puzzle)?
            && nft.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
        {
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
        } else if let Some((option, _)) = OptionInfo::parse(allocator, puzzle)?
            && option.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
        {
            self.options
                .entry(option.launcher_id)
                .or_default()
                .extend(notarized_payments);

            let info = OptionAssetInfo::new(
                option.underlying_coin_id,
                option.underlying_delegated_puzzle_hash,
            );
            asset_info.insert_option(option.launcher_id, info)?;
        } else {
            return Err(DriverError::UnsupportedRequestedAsset);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_types::puzzles::SettlementPayment;

    use super::*;

    #[test]
    fn test_unsupported_requested_assets_are_rejected() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let solution = ctx.alloc(&SettlementPaymentsSolution::new(vec![]))?;
        let settlement = ctx.alloc_mod::<SettlementPayment>()?;
        let unknown = ctx.alloc(&1)?;
        let asset_id = Bytes32::new([1; 32]);

        // A CAT sent to the settlement puzzle is supported
        let cat = CatInfo::new(asset_id, None, SETTLEMENT_PAYMENT_HASH.into())
            .construct_puzzle(&mut ctx, settlement)?;

        // A CAT with any other inner puzzle isn't a requested payment
        let unknown_cat = CatInfo::new(asset_id, None, ctx.tree_hash(unknown).into())
            .construct_puzzle(&mut ctx, unknown)?;

        let mut requested_payments = RequestedPayments::new();
        let mut asset_info = AssetInfo::new();

        let puzzle = Puzzle::parse(&ctx, cat);
        requested_payments.parse(&ctx, &mut asset_info, puzzle, solution)?;
        assert!(requested_payments.cats.contains_key(&asset_id));

        for puzzle in [unknown, unknown_cat] {
            let puzzle = Puzzle::parse(&ctx, puzzle);

            assert!(matches!(
                requested_payments.parse(&ctx, &mut asset_info, puzzle, solution),
                Err(DriverError::UnsupportedRequestedAsset)
            ));
        }

        Ok(())
    }
}
