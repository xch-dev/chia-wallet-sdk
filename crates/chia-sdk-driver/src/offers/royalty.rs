use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::conditions::TradePrice;

use crate::{AssetInfo, DriverError, OfferAmounts, RequestedPayments, SpendContext, coin_amount};

/// The royalty of an NFT, which must be paid when it's traded in an offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoyaltyInfo {
    /// The launcher id of the NFT, which is used as the nonce of the royalty payment.
    pub launcher_id: Bytes32,
    /// The puzzle hash that receives the royalty.
    pub puzzle_hash: Bytes32,
    /// The royalty percentage in basis points, so a value of 300 means 3%.
    pub basis_points: u16,
}

impl RoyaltyInfo {
    /// Creates a [`RoyaltyInfo`].
    pub fn new(launcher_id: Bytes32, puzzle_hash: Bytes32, basis_points: u16) -> Self {
        Self {
            launcher_id,
            puzzle_hash,
            basis_points,
        }
    }

    /// Creates the notarized payment of the royalty amount, with the launcher id as the nonce and a
    /// hint of the royalty puzzle hash.
    pub fn payment(
        &self,
        ctx: &mut SpendContext,
        amount: u64,
    ) -> Result<NotarizedPayment, DriverError> {
        let hint = ctx.hint(self.puzzle_hash)?;
        Ok(NotarizedPayment::new(
            self.launcher_id,
            vec![Payment::new(self.puzzle_hash, amount, hint)],
        ))
    }
}

/// Calculates the trade price of each royalty NFT, by splitting the amounts evenly between them.
///
/// This is empty if there are no royalty NFTs.
pub fn calculate_trade_price_amounts(
    amounts: &OfferAmounts,
    royalty_nft_count: usize,
) -> OfferAmounts {
    if royalty_nft_count == 0 {
        return OfferAmounts::new();
    }

    OfferAmounts {
        xch: calculate_nft_trade_price(amounts.xch, royalty_nft_count),
        cats: amounts
            .cats
            .iter()
            .map(|(&asset_id, &amount)| {
                let amount = calculate_nft_trade_price(amount, royalty_nft_count);
                (asset_id, amount)
            })
            .collect(),
    }
}

/// Converts trade price amounts into [`TradePrice`] values for transferring royalty NFTs, with
/// the settlement puzzle hash of each asset. Assets with a zero amount are skipped.
///
/// Fails with [`DriverError::AmountOverflow`] if a trade price doesn't fit in a [`u64`], or
/// [`DriverError::MissingAssetInfo`] if a CAT has no asset info.
pub fn calculate_trade_prices(
    trade_price_amounts: &OfferAmounts,
    asset_info: &AssetInfo,
) -> Result<Vec<TradePrice>, DriverError> {
    let mut trade_prices = Vec::new();

    if trade_price_amounts.xch > 0 {
        trade_prices.push(TradePrice::new(
            coin_amount(trade_price_amounts.xch)?,
            SETTLEMENT_PAYMENT_HASH.into(),
        ));
    }

    for (&asset_id, &amount) in &trade_price_amounts.cats {
        if amount == 0 {
            continue;
        }

        trade_prices.push(TradePrice::new(
            coin_amount(amount)?,
            asset_info.settlement_puzzle_hash(Some(asset_id))?,
        ));
    }

    Ok(trade_prices)
}

/// Returns the trade prices an NFT can reveal while still being possible to take, leaving out
/// any whose royalty rounds down to zero.
pub fn payable_trade_prices(
    trade_prices: &[TradePrice],
    royalty_basis_points: u16,
) -> Vec<TradePrice> {
    trade_prices
        .iter()
        .filter(|trade_price| calculate_nft_royalty(trade_price.amount, royalty_basis_points) > 0)
        .copied()
        .collect()
}

/// Calculates the royalty payment of each NFT for each asset of the trade price. Payments with a
/// zero amount are skipped.
///
/// Fails with [`DriverError::AmountOverflow`] if a trade price or royalty payment doesn't fit in
/// a [`u64`].
pub fn calculate_royalty_payments(
    ctx: &mut SpendContext,
    trade_prices: &OfferAmounts,
    royalties: &[RoyaltyInfo],
) -> Result<RequestedPayments, DriverError> {
    let mut payments = RequestedPayments::new();

    for royalty in royalties {
        let amount = coin_amount(calculate_nft_royalty(
            coin_amount(trade_prices.xch)?,
            royalty.basis_points,
        ))?;

        if amount > 0 {
            payments.xch.push(royalty.payment(ctx, amount)?);
        }

        for (&asset_id, &amount) in &trade_prices.cats {
            let amount = coin_amount(calculate_nft_royalty(
                coin_amount(amount)?,
                royalty.basis_points,
            ))?;

            if amount > 0 {
                payments
                    .cats
                    .entry(asset_id)
                    .or_default()
                    .push(royalty.payment(ctx, amount)?);
            }
        }
    }

    Ok(payments)
}

/// The total royalty owed for each asset. Fails with [`DriverError::AmountOverflow`] if a trade
/// price doesn't fit in a [`u64`], since it couldn't be used in a trade price condition.
pub fn calculate_royalty_amounts(
    trade_prices: &OfferAmounts,
    royalties: &[RoyaltyInfo],
) -> Result<OfferAmounts, DriverError> {
    let mut amounts = OfferAmounts::new();

    for royalty in royalties {
        amounts.xch += calculate_nft_royalty(coin_amount(trade_prices.xch)?, royalty.basis_points);

        for (&asset_id, &amount) in &trade_prices.cats {
            *amounts.cats.entry(asset_id).or_default() +=
                calculate_nft_royalty(coin_amount(amount)?, royalty.basis_points);
        }
    }

    Ok(amounts)
}

/// The trade price of each royalty NFT, which is zero if there are none.
pub fn calculate_nft_trade_price(amount: u128, royalty_nft_count: usize) -> u128 {
    amount.checked_div(royalty_nft_count as u128).unwrap_or(0)
}

/// Calculates the royalty of an NFT for the trade price, rounded down.
///
/// Royalties above 100% are representable on chain, so the royalty can exceed the trade price.
pub fn calculate_nft_royalty(trade_price: u64, royalty_basis_points: u16) -> u128 {
    u128::from(trade_price) * u128::from(royalty_basis_points) / 10_000
}

pub(crate) fn calculate_min_trade_price(royalty: u64, royalty_basis_points: u16) -> Option<u64> {
    if royalty_basis_points == 0 {
        return None;
    }

    let trade_price =
        u64::try_from((u128::from(royalty) * 10_000).div_ceil(u128::from(royalty_basis_points)))
            .ok()?;

    (calculate_nft_royalty(trade_price, royalty_basis_points) == u128::from(royalty))
        .then_some(trade_price)
}

#[cfg(test)]
mod tests;
