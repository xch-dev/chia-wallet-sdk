use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::conditions::TradePrice;

use crate::{
    AssetInfo, CatAssetInfo, CatInfo, DriverError, OfferAmounts, RequestedPayments, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn calculate_trade_prices(
    trade_price_amounts: &OfferAmounts,
    asset_info: &AssetInfo,
) -> Vec<TradePrice> {
    let mut trade_prices = Vec::new();

    if trade_price_amounts.xch > 0 {
        trade_prices.push(TradePrice::new(
            trade_price_amounts.xch,
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

        trade_prices.push(TradePrice::new(amount, puzzle_hash));
    }

    trade_prices
}

pub fn calculate_royalty_payments(
    ctx: &mut SpendContext,
    trade_prices: &OfferAmounts,
    royalties: &[RoyaltyInfo],
) -> Result<RequestedPayments, DriverError> {
    let mut payments = RequestedPayments::new();

    for royalty in royalties {
        let amount = calculate_nft_royalty(trade_prices.xch, royalty.basis_points);

        if amount > 0 {
            payments.xch.push(royalty.payment(ctx, amount)?);
        }

        for (&asset_id, &amount) in &trade_prices.cats {
            let amount = calculate_nft_royalty(amount, royalty.basis_points);

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

pub fn calculate_royalty_amounts(
    trade_prices: &OfferAmounts,
    royalties: &[RoyaltyInfo],
) -> OfferAmounts {
    let mut amounts = OfferAmounts::new();

    for royalty in royalties {
        amounts.xch = calculate_nft_royalty(trade_prices.xch, royalty.basis_points);

        for (&asset_id, &amount) in &trade_prices.cats {
            amounts.cats.insert(
                asset_id,
                calculate_nft_royalty(amount, royalty.basis_points),
            );
        }
    }

    amounts
}

pub fn calculate_nft_trace_price(amount: u64, royalty_nft_count: usize) -> u64 {
    let amount = BigDecimal::from(amount);
    let royalty_nft_count = BigDecimal::from(royalty_nft_count as u64);
    floor(amount / royalty_nft_count)
        .to_u64()
        .expect("out of bounds")
}

pub fn calculate_nft_royalty(trade_price: u64, royalty_percentage: u16) -> u64 {
    let trade_price = BigDecimal::from(trade_price);
    let royalty_percentage = BigDecimal::from(royalty_percentage);
    let percent = royalty_percentage / BigDecimal::from(10_000);
    floor(trade_price * percent)
        .to_u64()
        .expect("out of bounds")
}

#[allow(clippy::needless_pass_by_value)]
fn floor(amount: BigDecimal) -> BigDecimal {
    amount.with_scale_round(0, RoundingMode::Floor)
}
