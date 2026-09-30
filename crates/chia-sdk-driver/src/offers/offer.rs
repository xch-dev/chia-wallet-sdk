use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::TradePrice, puzzles::SettlementPayment, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;
use indexmap::IndexSet;

use crate::{
    Arbitrage, AssetInfo, CatInfo, DriverError, Id, Layer, NftInfo, OfferAmounts, OfferCoins,
    OptionInfo, Puzzle, RequestedPayments, RoyaltyInfo, SingletonInfo, SpendContext,
    calculate_min_trade_price, calculate_nft_royalty, coin_amount,
};

/// The tree hash of the retired `settlement_payments_old.clsp` puzzle. Offers that use it are
/// insecure, so they are rejected when parsed.
pub const OLD_SETTLEMENT_PAYMENT_HASH: [u8; 32] =
    hex!("bae24162efbd568f89bc7a340798a6118df0189eb9e3f8697bcea27af99f8f79");

#[derive(Debug, Clone)]
pub struct Offer {
    spend_bundle: SpendBundle,
    offered_coins: OfferCoins,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
}

impl Offer {
    pub fn new(
        spend_bundle: SpendBundle,
        offered_coins: OfferCoins,
        requested_payments: RequestedPayments,
        asset_info: AssetInfo,
    ) -> Self {
        Self {
            spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        }
    }

    pub fn cancellable_coin_spends(&self) -> Result<Vec<CoinSpend>, DriverError> {
        let mut allocator = Allocator::new();
        let mut created_coin_ids = HashSet::new();

        for coin_spend in &self.spend_bundle.coin_spends {
            let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
            let solution = coin_spend.solution.to_clvm(&mut allocator)?;

            let output = run_puzzle(&mut allocator, puzzle, solution)?;
            let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

            for condition in conditions {
                if let Some(create_coin) = condition.into_create_coin() {
                    created_coin_ids.insert(
                        Coin::new(
                            coin_spend.coin.coin_id(),
                            create_coin.puzzle_hash,
                            create_coin.amount,
                        )
                        .coin_id(),
                    );
                }
            }
        }

        Ok(self
            .spend_bundle
            .coin_spends
            .iter()
            .filter_map(|cs| {
                if created_coin_ids.contains(&cs.coin.coin_id()) {
                    None
                } else {
                    Some(cs.clone())
                }
            })
            .collect())
    }

    /// The amount that the offer's spends don't assign to any coin, which goes to the farmer as a
    /// fee. Unlike [`OfferCoins::fee`], this doesn't rely on `RESERVE_FEE` conditions.
    ///
    /// This is negative if the spends create more than they spend, in which case the offer can only
    /// be taken by a spend bundle that makes up the difference.
    pub fn fee(&self) -> Result<i128, DriverError> {
        let mut allocator = Allocator::new();
        let mut removals = 0;
        let mut additions = 0;

        for coin_spend in &self.spend_bundle.coin_spends {
            removals += i128::from(coin_spend.coin.amount);

            let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
            let solution = coin_spend.solution.to_clvm(&mut allocator)?;

            let output = run_puzzle(&mut allocator, puzzle, solution)?;
            let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

            for condition in conditions {
                if let Some(create_coin) = condition.into_create_coin() {
                    additions += i128::from(create_coin.amount);
                }
            }
        }

        Ok(removals - additions)
    }

    pub fn spend_bundle(&self) -> &SpendBundle {
        &self.spend_bundle
    }

    pub fn offered_coins(&self) -> &OfferCoins {
        &self.offered_coins
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    /// Returns the royalty info for requested NFTs, since those are the royalties
    /// that need to be paid by the offered side.
    ///
    /// Returns [`DriverError::MissingAssetInfo`] if a requested NFT has no asset info, since its
    /// royalty would otherwise be skipped.
    pub fn offered_royalties(&self) -> Result<Vec<RoyaltyInfo>, DriverError> {
        let mut royalties = Vec::new();

        for &launcher_id in self.requested_payments.nfts.keys() {
            let nft = self
                .asset_info
                .nft(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            if nft.royalty_basis_points > 0 {
                royalties.push(RoyaltyInfo::new(
                    launcher_id,
                    nft.royalty_puzzle_hash,
                    nft.royalty_basis_points,
                ));
            }
        }

        Ok(royalties)
    }

    /// Returns the royalty info for offered NFTs, since those are the royalties
    /// that need to be paid by the requested side.
    pub fn requested_royalties(&self) -> Vec<RoyaltyInfo> {
        self.offered_coins
            .nfts
            .values()
            .map(|nft| {
                RoyaltyInfo::new(
                    nft.info.launcher_id,
                    nft.info.royalty_puzzle_hash,
                    nft.info.royalty_basis_points,
                )
            })
            .filter(|royalty| royalty.basis_points > 0)
            .collect()
    }

    /// Returns the royalties already paid in the offer for requested NFTs.
    pub fn offered_royalty_amounts(&self) -> Result<OfferAmounts, DriverError> {
        let mut amounts = OfferAmounts::new();

        for &launcher_id in self.requested_payments.nfts.keys() {
            for (asset, amount) in self.prepaid_royalties(launcher_id) {
                amounts.add_amount(asset, amount.into());
            }
        }

        Ok(amounts)
    }

    /// Returns the royalties that need to be paid for offered NFTs.
    ///
    /// Trade prices whose royalty can't be paid are left out, see [`Offer::requested_royalty_payments`].
    pub fn requested_royalty_amounts(&self) -> Result<OfferAmounts, DriverError> {
        let mut amounts = OfferAmounts::new();

        for committed in self.committed_royalties() {
            if let Some(asset) = committed.asset
                && committed.amount > 0
            {
                amounts.add_amount(asset, committed.amount);
            }
        }

        Ok(amounts)
    }

    /// Returns the royalty payments that need to be settled for offered NFTs.
    ///
    /// Each offered NFT asserts a royalty payment for every trade price it revealed when it was
    /// spent into settlement. Since those trade prices are chosen by each maker, they can't be
    /// derived from the totals of an aggregated offer. Identical payments are only included once,
    /// since a single announcement satisfies all of the NFT's assertions for it.
    ///
    /// Settlement payments must be positive, so this fails with
    /// [`DriverError::ZeroRoyaltyPayment`] if a trade price's royalty rounds down to zero. It fails
    /// with [`DriverError::AmountOverflow`] if a royalty doesn't fit in a [`u64`].
    pub fn requested_royalty_payments(
        &self,
        ctx: &mut SpendContext,
    ) -> Result<RequestedPayments, DriverError> {
        let mut payments = RequestedPayments::new();

        for CommittedRoyalty {
            royalty,
            settlement_puzzle_hash,
            asset,
            amount,
        } in self.committed_royalties()
        {
            let asset = asset.ok_or(DriverError::UnknownTradePriceAsset(settlement_puzzle_hash))?;

            if amount == 0 {
                return Err(DriverError::ZeroRoyaltyPayment(royalty.launcher_id));
            }

            payments
                .fungible_mut(asset)
                .push(royalty.payment(ctx, coin_amount(amount)?)?);
        }

        Ok(payments)
    }

    /// Returns the trade prices a requested NFT should reveal when the taker spends it into
    /// settlement, so that it asserts exactly the royalties the offer already paid for it.
    ///
    /// Prepaid royalties that no trade price could produce are left out, since the NFT can't
    /// assert them.
    ///
    /// An NFT that's also offered only passes through settlement, so the taker shouldn't
    /// transfer it or reveal any trade prices for it.
    pub fn requested_nft_trade_prices(&self, launcher_id: Bytes32) -> Vec<TradePrice> {
        let Some(nft) = self.asset_info.nft(launcher_id) else {
            return Vec::new();
        };

        self.prepaid_royalties(launcher_id)
            .filter_map(|(asset, royalty)| {
                let amount = calculate_min_trade_price(royalty, nft.royalty_basis_points)?;
                Some(TradePrice::new(
                    amount,
                    self.asset_info.settlement_puzzle_hash(asset),
                ))
            })
            .collect()
    }

    /// Each distinct royalty payment asserted by offered NFTs.
    fn committed_royalties(&self) -> IndexSet<CommittedRoyalty> {
        self.offered_coins
            .nfts
            .iter()
            .flat_map(|(&launcher_id, nft)| {
                let royalty = RoyaltyInfo::new(
                    launcher_id,
                    nft.info.royalty_puzzle_hash,
                    nft.info.royalty_basis_points,
                );

                self.offered_coins
                    .nft_trade_prices
                    .get(&launcher_id)
                    .into_iter()
                    .flatten()
                    .map(move |trade_price| CommittedRoyalty {
                        royalty,
                        settlement_puzzle_hash: trade_price.puzzle_hash,
                        asset: self.asset_info.settlement_asset(trade_price.puzzle_hash),
                        amount: calculate_nft_royalty(trade_price.amount, royalty.basis_points),
                    })
            })
            .collect()
    }

    /// Royalty payments for a requested NFT made by settlement spends in the offer, along with
    /// the asset they're paid in.
    fn prepaid_royalties(&self, launcher_id: Bytes32) -> impl Iterator<Item = (Id, u64)> {
        let royalty_puzzle_hash = self
            .asset_info
            .nft(launcher_id)
            .map(|nft| nft.royalty_puzzle_hash);

        self.offered_coins
            .settled_payments
            .fungible()
            .filter(move |(_, notarized_payment)| notarized_payment.nonce == launcher_id)
            .flat_map(move |(asset, notarized_payment)| {
                notarized_payment
                    .payments
                    .iter()
                    .filter(move |payment| Some(payment.puzzle_hash) == royalty_puzzle_hash)
                    .map(move |payment| (asset, payment.amount))
            })
    }

    pub fn arbitrage(&self) -> Arbitrage {
        let offered = self.offered_coins.amounts();
        let requested = self.requested_payments.amounts();

        let mut arbitrage = Arbitrage::new();

        if requested.xch > offered.xch {
            arbitrage.offered.xch = requested.xch - offered.xch;
        } else {
            arbitrage.requested.xch = offered.xch - requested.xch;
        }

        for &asset_id in offered
            .cats
            .keys()
            .chain(requested.cats.keys())
            .collect::<IndexSet<_>>()
        {
            let &offered_amount = offered.cats.get(&asset_id).unwrap_or(&0);
            let &requested_amount = requested.cats.get(&asset_id).unwrap_or(&0);

            if requested_amount > offered_amount {
                let diff = requested_amount - offered_amount;
                arbitrage.offered.cats.insert(asset_id, diff);
            } else {
                let diff = offered_amount - requested_amount;
                arbitrage.requested.cats.insert(asset_id, diff);
            }
        }

        for &launcher_id in self
            .offered_coins
            .nfts
            .keys()
            .chain(self.requested_payments.nfts.keys())
            .collect::<IndexSet<_>>()
        {
            let is_offered = self.offered_coins.nfts.contains_key(&launcher_id);
            let is_requested = self.requested_payments.nfts.contains_key(&launcher_id);

            if is_offered && !is_requested {
                arbitrage.requested.nfts.push(launcher_id);
            } else if !is_offered && is_requested {
                arbitrage.offered.nfts.push(launcher_id);
            }
        }

        for &launcher_id in self
            .offered_coins
            .options
            .keys()
            .chain(self.requested_payments.options.keys())
            .collect::<IndexSet<_>>()
        {
            let is_offered = self.offered_coins.options.contains_key(&launcher_id);
            let is_requested = self.requested_payments.options.contains_key(&launcher_id);

            if is_offered && !is_requested {
                arbitrage.requested.options.push(launcher_id);
            } else if !is_offered && is_requested {
                arbitrage.offered.options.push(launcher_id);
            }
        }

        arbitrage
    }

    /// The nonce that requested payments are notarized with, which is the tree hash of the sorted
    /// ids of the coins being offered (ie, the coins spent by the maker which aren't created in the
    /// offer). Since each coin can only be spent once, no two offers that can both be taken share a
    /// nonce, so a taker can't combine offers and pay for both with the same payment.
    pub fn nonce(mut coin_ids: Vec<Bytes32>) -> Bytes32 {
        coin_ids.sort();
        coin_ids.tree_hash().into()
    }

    /// Creates an offer from the maker's own spends. Unlike [`Offer::from_spend_bundle`], the puzzle
    /// reveals aren't checked against the coins' puzzle hashes, since the spends are trusted and may
    /// still contain placeholders that are replaced when they're signed.
    pub fn from_input_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: SpendBundle,
        requested_payments: RequestedPayments,
        requested_asset_info: AssetInfo,
    ) -> Result<Self, DriverError> {
        let mut offered_coins = OfferCoins::new();
        let mut asset_info = requested_asset_info;

        let spent_coin_ids: HashSet<Bytes32> = spend_bundle
            .coin_spends
            .iter()
            .map(|cs| cs.coin.coin_id())
            .collect();

        for coin_spend in &spend_bundle.coin_spends {
            let (puzzle, solution) = deserialize_coin_spend(allocator, coin_spend)?;

            offered_coins.parse(
                allocator,
                &mut asset_info,
                &spent_coin_ids,
                coin_spend.coin,
                puzzle,
                solution,
            )?;
        }

        Ok(Self::new(
            spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        ))
    }

    /// Parses an untrusted offer spend bundle. The spends of coins with a parent coin id of zero are
    /// the requested payments, and the rest are the maker's spends.
    ///
    /// Returns [`DriverError::WrongPuzzleHash`] if the puzzle reveal of a spend doesn't match the
    /// coin's puzzle hash.
    pub fn from_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: &SpendBundle,
    ) -> Result<Self, DriverError> {
        let mut input_spend_bundle =
            SpendBundle::new(Vec::new(), spend_bundle.aggregated_signature.clone());
        let mut offered_coins = OfferCoins::new();
        let mut requested_payments = RequestedPayments::new();
        let mut asset_info = AssetInfo::new();

        let spent_coin_ids: HashSet<Bytes32> = spend_bundle
            .coin_spends
            .iter()
            .filter_map(|cs| {
                if cs.coin.parent_coin_info == Bytes32::default() {
                    None
                } else {
                    Some(cs.coin.coin_id())
                }
            })
            .collect();

        for coin_spend in &spend_bundle.coin_spends {
            let (puzzle, solution) = parse_coin_spend(allocator, coin_spend)?;

            if coin_spend.coin.parent_coin_info == Bytes32::default() {
                check_not_old_settlement_payment(allocator, puzzle)?;
                requested_payments.parse(allocator, &mut asset_info, puzzle, solution)?;
            } else {
                input_spend_bundle.coin_spends.push(coin_spend.clone());

                offered_coins.parse(
                    allocator,
                    &mut asset_info,
                    &spent_coin_ids,
                    coin_spend.coin,
                    puzzle,
                    solution,
                )?;
            }
        }

        Ok(Self::new(
            input_spend_bundle,
            offered_coins,
            requested_payments,
            asset_info,
        ))
    }

    /// The offer spend bundle, with a spend for each kind of requested payment. These come before
    /// the maker's spends, as in the reference wallet, so that an offer it made encodes the same
    /// after being parsed.
    pub fn to_spend_bundle(self, ctx: &mut SpendContext) -> Result<SpendBundle, DriverError> {
        let settlement = ctx.alloc_mod::<SettlementPayment>()?;
        let mut coin_spends = Vec::new();

        if !self.requested_payments.xch.is_empty() {
            let solution = SettlementPaymentsSolution::new(self.requested_payments.xch);

            coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), SETTLEMENT_PAYMENT_HASH.into(), 0),
                ctx.serialize(&settlement)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (asset_id, notarized_payments) in self.requested_payments.cats {
            let info = self
                .asset_info
                .cat(asset_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let cat_info = CatInfo::new(
                asset_id,
                info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = cat_info.construct_puzzle(ctx, settlement)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), cat_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (launcher_id, notarized_payments) in self.requested_payments.nfts {
            let info = self
                .asset_info
                .nft(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let nft_info = NftInfo::new(
                launcher_id,
                info.metadata,
                info.metadata_updater_puzzle_hash,
                None,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = nft_info.into_layers(settlement).construct_puzzle(ctx)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), nft_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        for (launcher_id, notarized_payments) in self.requested_payments.options {
            let info = self
                .asset_info
                .option(launcher_id)
                .ok_or(DriverError::MissingAssetInfo)?;

            let option_info = OptionInfo::new(
                launcher_id,
                info.underlying_coin_id,
                info.underlying_delegated_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            );

            let puzzle = option_info.into_layers(settlement).construct_puzzle(ctx)?;
            let solution = SettlementPaymentsSolution::new(notarized_payments);

            coin_spends.push(CoinSpend::new(
                Coin::new(Bytes32::default(), option_info.puzzle_hash().into(), 0),
                ctx.serialize(&puzzle)?,
                ctx.serialize(&solution)?,
            ));
        }

        coin_spends.extend(self.spend_bundle.coin_spends);

        Ok(SpendBundle::new(
            coin_spends,
            self.spend_bundle.aggregated_signature,
        ))
    }

    pub fn extend(&mut self, other: Self) -> Result<(), DriverError> {
        self.spend_bundle
            .coin_spends
            .extend(other.spend_bundle.coin_spends);
        self.spend_bundle.aggregated_signature += &other.spend_bundle.aggregated_signature;
        self.offered_coins.extend(other.offered_coins)?;
        self.requested_payments.extend(other.requested_payments);
        self.asset_info.extend(other.asset_info)?;

        Ok(())
    }

    pub fn take(self, spend_bundle: SpendBundle) -> SpendBundle {
        SpendBundle::new(
            [self.spend_bundle.coin_spends, spend_bundle.coin_spends].concat(),
            self.spend_bundle.aggregated_signature + &spend_bundle.aggregated_signature,
        )
    }
}

/// Deserializes the puzzle and solution of an untrusted coin spend, and checks that the puzzle
/// reveal matches the coin's puzzle hash.
fn parse_coin_spend(
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
) -> Result<(Puzzle, NodePtr), DriverError> {
    let (puzzle, solution) = deserialize_coin_spend(allocator, coin_spend)?;

    if puzzle.curried_puzzle_hash() != coin_spend.coin.puzzle_hash.into() {
        return Err(DriverError::WrongPuzzleHash);
    }

    Ok((puzzle, solution))
}

/// Deserializes the puzzle and solution of a trusted coin spend.
fn deserialize_coin_spend(
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
) -> Result<(Puzzle, NodePtr), DriverError> {
    let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
    let puzzle = Puzzle::parse(allocator, puzzle);
    let solution = coin_spend.solution.to_clvm(allocator)?;

    Ok((puzzle, solution))
}

/// Requested payments are made by settlement coins. Anything other than the settlement payments puzzle
/// is rejected when the requested payments are parsed, but offers made with the old settlement
/// payments puzzle get their own error so that wallets can explain why they can't be taken.
fn check_not_old_settlement_payment(
    allocator: &Allocator,
    puzzle: Puzzle,
) -> Result<(), DriverError> {
    let p2_puzzle_hash = if let Some((cat, _)) = CatInfo::parse(allocator, puzzle)? {
        cat.p2_puzzle_hash
    } else if let Some((nft, _)) = NftInfo::parse(allocator, puzzle)? {
        nft.p2_puzzle_hash
    } else if let Some((option, _)) = OptionInfo::parse(allocator, puzzle)? {
        option.p2_puzzle_hash
    } else {
        puzzle.curried_puzzle_hash().into()
    };

    if p2_puzzle_hash == OLD_SETTLEMENT_PAYMENT_HASH.into() {
        return Err(DriverError::OldSettlementPayment);
    }

    Ok(())
}

/// A royalty payment an offered NFT asserts for one of the trade prices it revealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CommittedRoyalty {
    royalty: RoyaltyInfo,
    settlement_puzzle_hash: Bytes32,
    /// The asset paid to the settlement puzzle hash, if it's known.
    asset: Option<Id>,
    amount: u128,
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_bls::Signature;
    use chia_puzzle_types::{
        Memos,
        offer::{NotarizedPayment, Payment},
    };
    use chia_sdk_test::{Simulator, sign_transaction};
    use indexmap::indexmap;

    use crate::{Action, NftAssetInfo, Relation, SpendContext, Spends};

    use super::*;

    #[cfg(feature = "offer-compression")]
    mod parsing {
        use chia_protocol::Program;
        use clvm_utils::tree_hash_from_bytes;

        use crate::{decode_offer, encode_offer};

        use super::*;

        const XCH_FOR_CAT: &str = include_str!("./test_data/xch_for_cat.offer");
        const CAT_FOR_XCH: &str = include_str!("./test_data/cat_for_xch.offer");
        const NFT_FOR_XCH: &str = include_str!("./test_data/nft_for_xch.offer");
        const REVOCABLE_CAT_FOR_XCH: &str = include_str!("./test_data/revocable_cat_for_xch.offer");

        const SBX: Bytes32 = Bytes32::new(hex!(
            "a628c1c2c6fcb74d53746157e438e108eab5c0bb3e5c80ff9b1910b3e4832913"
        ));
        const REVOCABLE_CAT: Bytes32 = Bytes32::new(hex!(
            "1a14caeec10ef11b071af9962d8ada0e00b61fa2d42ac4bdc646e70ef34e827e"
        ));
        const NFT: Bytes32 = Bytes32::new(hex!(
            "10abc84fc9fdc5bd917d84b26efb412daa6139a3cb119c192871aba2fa92b714"
        ));

        fn parse(offer: &str) -> anyhow::Result<Offer> {
            let mut allocator = Allocator::new();
            Ok(Offer::from_spend_bundle(
                &mut allocator,
                &decode_offer(offer.trim())?,
            )?)
        }

        fn replace(bytes: &[u8], from: &[u8; 32], to: &[u8; 32]) -> Vec<u8> {
            let mut result = bytes.to_vec();
            let mut index = 0;

            while index + 32 <= result.len() {
                if result[index..index + 32] == from[..] {
                    result[index..index + 32].copy_from_slice(to);
                    index += 32;
                } else {
                    index += 1;
                }
            }

            result
        }

        /// The offer as it would be with the old settlement payments puzzle, optionally without its
        /// requested payments.
        fn with_old_settlement(offer: &str, requested: bool) -> anyhow::Result<SpendBundle> {
            let spend_bundle = decode_offer(offer.trim())?;

            let coin_spends = spend_bundle
                .coin_spends
                .into_iter()
                .filter(|cs| requested || cs.coin.parent_coin_info != Bytes32::default())
                .map(|cs| {
                    let puzzle_reveal = replace(
                        cs.puzzle_reveal.as_ref(),
                        &SETTLEMENT_PAYMENT_HASH,
                        &OLD_SETTLEMENT_PAYMENT_HASH,
                    );

                    // The coin's puzzle hash has to match the new puzzle reveal
                    let coin = Coin::new(
                        cs.coin.parent_coin_info,
                        tree_hash_from_bytes(&puzzle_reveal)?.into(),
                        cs.coin.amount,
                    );

                    Ok(CoinSpend::new(
                        coin,
                        Program::from(puzzle_reveal),
                        Program::from(replace(
                            cs.solution.as_ref(),
                            &SETTLEMENT_PAYMENT_HASH,
                            &OLD_SETTLEMENT_PAYMENT_HASH,
                        )),
                    ))
                })
                .collect::<anyhow::Result<_>>()?;

            Ok(SpendBundle::new(
                coin_spends,
                spend_bundle.aggregated_signature,
            ))
        }

        #[test]
        fn test_round_trip() -> anyhow::Result<()> {
            for offer in [XCH_FOR_CAT, CAT_FOR_XCH, NFT_FOR_XCH, REVOCABLE_CAT_FOR_XCH] {
                let mut ctx = SpendContext::new();
                let spend_bundle = decode_offer(offer.trim())?;
                let offer = Offer::from_spend_bundle(&mut ctx, &spend_bundle)?;
                assert_eq!(offer.to_spend_bundle(&mut ctx)?, spend_bundle);
            }

            // The revocable CAT offer was compressed with an older dictionary
            for offer in [XCH_FOR_CAT, CAT_FOR_XCH, NFT_FOR_XCH] {
                let mut ctx = SpendContext::new();
                let spend_bundle = decode_offer(offer.trim())?;
                let offer_spend_bundle =
                    Offer::from_spend_bundle(&mut ctx, &spend_bundle)?.to_spend_bundle(&mut ctx)?;
                assert_eq!(encode_offer(&offer_spend_bundle)?, offer.trim());
            }

            Ok(())
        }

        #[test]
        fn test_input_spend_bundle() -> anyhow::Result<()> {
            let offer = parse(XCH_FOR_CAT)?;

            assert_eq!(offer.spend_bundle().coin_spends.len(), 1);
            assert_eq!(
                offer.spend_bundle().coin_spends[0].coin.coin_id(),
                Bytes32::new(hex!(
                    "cbe9b5df17ebc0647a30cb91e2e3bb3f63342c34ada849526baa80212e023503"
                ))
            );
            assert_eq!(
                offer.cancellable_coin_spends()?,
                offer.spend_bundle().coin_spends
            );
            assert_eq!(offer.fee()?, 0);

            let requested = &offer.requested_payments().cats[&SBX];
            assert_eq!(requested.len(), 1);
            assert_eq!(requested[0].payments[0].amount, 35_500_000);

            Ok(())
        }

        #[test]
        fn test_arbitrage() -> anyhow::Result<()> {
            let arbitrage = parse(XCH_FOR_CAT)?.arbitrage();
            assert_eq!(arbitrage.requested.xch, 1_000_000_000_000);
            assert_eq!(arbitrage.offered.cats[&SBX], 35_500_000);

            let arbitrage = parse(CAT_FOR_XCH)?.arbitrage();
            assert_eq!(arbitrage.offered.xch, 2_000_000_000_000);
            assert_eq!(arbitrage.requested.cats[&SBX], 69_400_000);

            let arbitrage = parse(NFT_FOR_XCH)?.arbitrage();
            assert_eq!(arbitrage.offered.xch, 9_000_000_000_000);
            assert_eq!(arbitrage.requested.nfts, vec![NFT]);

            let arbitrage = parse(REVOCABLE_CAT_FOR_XCH)?.arbitrage();
            assert_eq!(arbitrage.offered.xch, 2);
            assert_eq!(arbitrage.requested.cats[&REVOCABLE_CAT], 5000);

            Ok(())
        }

        #[test]
        fn test_old_settlement_payment() -> anyhow::Result<()> {
            for offer in [XCH_FOR_CAT, CAT_FOR_XCH, NFT_FOR_XCH, REVOCABLE_CAT_FOR_XCH] {
                for requested in [true, false] {
                    let mut allocator = Allocator::new();
                    let spend_bundle = with_old_settlement(offer, requested)?;

                    assert!(matches!(
                        Offer::from_spend_bundle(&mut allocator, &spend_bundle),
                        Err(DriverError::OldSettlementPayment)
                    ));
                }
            }

            Ok(())
        }
    }

    #[test]
    fn test_offer_nft_for_nft() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = sim.bls(0);

        let alice_hint = ctx.hint(alice.puzzle_hash)?;
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        // Mint NFTs
        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::mint_empty_royalty_nft(alice.puzzle_hash, 300),
                Action::mint_empty_royalty_nft(bob.puzzle_hash, 300),
                Action::send(Id::New(1), bob.puzzle_hash, 1, bob_hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let alice_nft = outputs.nfts[&Id::New(0)];
        let bob_nft = outputs.nfts[&Id::New(1)];

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Make offer
        let mut requested_payments = RequestedPayments::new();
        let mut requested_asset_info = AssetInfo::new();

        requested_payments.nfts.insert(
            bob_nft.info.launcher_id,
            vec![NotarizedPayment::new(
                Offer::nonce(vec![alice_nft.coin.coin_id()]),
                vec![Payment::new(alice.puzzle_hash, 1, alice_hint)],
            )],
        );
        requested_asset_info.insert_nft(
            bob_nft.info.launcher_id,
            NftAssetInfo::new(
                bob_nft.info.metadata,
                bob_nft.info.metadata_updater_puzzle_hash,
                bob_nft.info.royalty_puzzle_hash,
                bob_nft.info.royalty_basis_points,
            ),
        )?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice_nft);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::send(
                Id::Existing(alice_nft.info.launcher_id),
                SETTLEMENT_PAYMENT_HASH.into(),
                1,
                Memos::None,
            )],
        )?;

        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested_payments.assertions(&mut ctx, &requested_asset_info)?);

        spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[alice.sk])?;

        let offer = Offer::from_input_spend_bundle(
            &mut ctx,
            SpendBundle::new(coin_spends, signature),
            requested_payments,
            requested_asset_info,
        )?;

        // Take offer
        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(offer.offered_coins().clone());
        spends.add(bob_nft);

        let deltas = spends.apply(&mut ctx, &offer.requested_payments().actions())?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[bob.sk])?;

        let spend_bundle = offer.take(SpendBundle::new(coin_spends, signature));

        sim.new_transaction(spend_bundle)?;

        let final_bob_nft = outputs.nfts[&Id::Existing(alice_nft.info.launcher_id)];
        let final_alice_nft = outputs.nfts[&Id::Existing(bob_nft.info.launcher_id)];

        assert_eq!(final_bob_nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(final_alice_nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[test]
    fn test_puzzle_reveal_must_match_puzzle_hash() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let puzzle = ctx.alloc(&1)?;
        let puzzle = ctx.serialize(&puzzle)?;
        let solution = ctx.serialize(&NodePtr::NIL)?;

        // Both the requested payments (with a parent coin id of zero) and the maker's spends are checked
        for parent_coin_id in [Bytes32::default(), Bytes32::new([1; 32])] {
            let coin = Coin::new(parent_coin_id, Bytes32::new([2; 32]), 1);
            let spend_bundle = SpendBundle::new(
                vec![CoinSpend::new(coin, puzzle.clone(), solution.clone())],
                Signature::default(),
            );

            assert!(matches!(
                Offer::from_spend_bundle(&mut ctx, &spend_bundle),
                Err(DriverError::WrongPuzzleHash)
            ));
        }

        // The maker's own spends are trusted, and may contain placeholders until they're signed
        let coin = Coin::new(Bytes32::new([1; 32]), Bytes32::new([2; 32]), 1);
        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle, solution)],
            Signature::default(),
        );

        Offer::from_input_spend_bundle(
            &mut ctx,
            spend_bundle,
            RequestedPayments::new(),
            AssetInfo::new(),
        )?;

        Ok(())
    }
}
