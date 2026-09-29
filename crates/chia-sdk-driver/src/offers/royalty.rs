use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::conditions::TradePrice;

use crate::{
    AssetInfo, CatAssetInfo, CatInfo, DriverError, OfferAmounts, RequestedPayments, SpendContext,
    coin_amount,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoyaltyInfo {
    pub launcher_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub basis_points: u16,
}

impl RoyaltyInfo {
    pub fn new(launcher_id: Bytes32, puzzle_hash: Bytes32, basis_points: u16) -> Self {
        Self {
            launcher_id,
            puzzle_hash,
            basis_points,
        }
    }

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

pub fn calculate_trade_price_amounts(
    amounts: &OfferAmounts,
    royalty_nft_count: usize,
) -> OfferAmounts {
    if royalty_nft_count == 0 {
        return OfferAmounts::new();
    }

    OfferAmounts {
        xch: calculate_nft_trace_price(amounts.xch, royalty_nft_count),
        cats: amounts
            .cats
            .iter()
            .map(|(&asset_id, &amount)| {
                let amount = calculate_nft_trace_price(amount, royalty_nft_count);
                (asset_id, amount)
            })
            .collect(),
    }
}

/// Fails with [`DriverError::AmountOverflow`] if a trade price doesn't fit in a [`u64`].
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

        let default = CatAssetInfo::default();
        let info = asset_info.cat(asset_id).unwrap_or(&default);
        let puzzle_hash = CatInfo::new(
            asset_id,
            info.hidden_puzzle_hash,
            SETTLEMENT_PAYMENT_HASH.into(),
        )
        .puzzle_hash()
        .into();

        trade_prices.push(TradePrice::new(coin_amount(amount)?, puzzle_hash));
    }

    Ok(trade_prices)
}

pub fn payable_trade_prices(
    trade_prices: &[TradePrice],
    _royalty_basis_points: u16,
) -> Vec<TradePrice> {
    trade_prices.to_vec()
}

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
pub fn calculate_nft_trace_price(amount: u128, royalty_nft_count: usize) -> u128 {
    amount.checked_div(royalty_nft_count as u128).unwrap_or(0)
}

/// Royalties above 100% are representable on chain, so the royalty can exceed the trade price.
pub fn calculate_nft_royalty(trade_price: u64, royalty_basis_points: u16) -> u128 {
    u128::from(trade_price) * u128::from(royalty_basis_points) / 10_000
}
