use chia_protocol::Bytes32;
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
};
use chia_sdk_types::puzzles::FeeTradePrice;
use clvmr::NodePtr;

use crate::{DriverError, FeePolicy, OfferAmounts, RequestedPayments, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferFeeInfo {
    pub asset_id: Bytes32,
    pub policy: FeePolicy,
}

impl TransferFeeInfo {
    pub fn new(asset_id: Bytes32, policy: FeePolicy) -> Self {
        Self { asset_id, policy }
    }
}

pub fn calculate_transfer_fee(amount: u64, policy: &FeePolicy) -> u64 {
    if amount == 0 {
        return 0;
    }

    let proportional = (u128::from(amount) * u128::from(policy.fee_basis_points)) / 10_000;
    let proportional = u64::try_from(proportional).expect("transfer fee overflow");
    proportional.max(policy.min_fee)
}

pub fn calculate_transfer_fee_amounts(
    trade_prices: &OfferAmounts,
    transfer_fees: &[TransferFeeInfo],
) -> OfferAmounts {
    let mut amounts = OfferAmounts::new();

    for transfer_fee in transfer_fees {
        amounts.xch += calculate_transfer_fee(trade_prices.xch, &transfer_fee.policy);

        for (&asset_id, &amount) in &trade_prices.cats {
            let fee = calculate_transfer_fee(amount, &transfer_fee.policy);
            *amounts.cats.entry(asset_id).or_default() += fee;
        }
    }

    amounts
}

pub fn calculate_transfer_fee_payments(
    ctx: &mut SpendContext,
    trade_nonce: Bytes32,
    trade_prices: &OfferAmounts,
    transfer_fees: &[TransferFeeInfo],
) -> Result<RequestedPayments, DriverError> {
    let mut payments = RequestedPayments::new();

    for transfer_fee in transfer_fees {
        let xch_fee = calculate_transfer_fee(trade_prices.xch, &transfer_fee.policy);
        if xch_fee > 0 {
            payments.xch.push(NotarizedPayment::new(
                trade_nonce,
                vec![Payment::new(
                    transfer_fee.policy.issuer_fee_puzzle_hash,
                    xch_fee,
                    Memos::Some(NodePtr::NIL),
                )],
            ));
        }

        let hint = ctx.hint(transfer_fee.policy.issuer_fee_puzzle_hash)?;
        for (&asset_id, &amount) in &trade_prices.cats {
            let cat_fee = calculate_transfer_fee(amount, &transfer_fee.policy);
            if cat_fee == 0 {
                continue;
            }

            payments
                .cats
                .entry(asset_id)
                .or_default()
                .push(NotarizedPayment::new(
                    trade_nonce,
                    vec![Payment::new(
                        transfer_fee.policy.issuer_fee_puzzle_hash,
                        cat_fee,
                        hint.clone(),
                    )],
                ));
        }
    }

    Ok(payments)
}

pub fn ensure_trade_prices_supported(trade_prices: &[FeeTradePrice]) -> Result<(), DriverError> {
    for trade_price in trade_prices {
        if !trade_price.is_valid_quote_descriptor() {
            return Err(DriverError::InvalidTradePriceProfile);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes32;
    use chia_sdk_types::puzzles::FeeTradePriceFeePolicy;
    use indexmap::indexmap;

    use super::*;

    #[test]
    fn test_calculate_transfer_fee() {
        let policy = FeePolicy::new(
            Bytes32::new([1; 32]),
            500,
            3,
            false,
            false,
        );

        assert_eq!(calculate_transfer_fee(0, &policy), 0);
        assert_eq!(calculate_transfer_fee(1, &policy), 3);
        assert_eq!(calculate_transfer_fee(1_000, &policy), 50);
    }

    #[test]
    fn test_calculate_transfer_fee_amounts() {
        let trade_prices = OfferAmounts {
            xch: 1_000,
            cats: indexmap! {
                Bytes32::new([2; 32]) => 300,
            },
        };
        let transfer_fees = vec![
            TransferFeeInfo::new(
                Bytes32::new([3; 32]),
                FeePolicy::new(
                    Bytes32::new([4; 32]),
                    500,
                    1,
                    false,
                    false,
                ),
            ),
            TransferFeeInfo::new(
                Bytes32::new([5; 32]),
                FeePolicy::new(
                    Bytes32::new([6; 32]),
                    250,
                    0,
                    false,
                    false,
                ),
            ),
        ];

        let amounts = calculate_transfer_fee_amounts(&trade_prices, &transfer_fees);
        assert_eq!(amounts.xch, 75);
        assert_eq!(amounts.cats[&Bytes32::new([2; 32])], 22);
    }

    #[test]
    fn test_ensure_trade_prices_supported_accepts_valid_trade_prices() {
        let trade_prices = vec![FeeTradePrice::xch(200)];

        assert!(ensure_trade_prices_supported(&trade_prices).is_ok());
    }

    #[test]
    fn test_ensure_trade_prices_supported_accepts_valid_cat_trade_price() {
        let trade_prices = vec![FeeTradePrice::cat(
            200,
            Bytes32::new([7; 32]),
            None,
            None,
        )];
        assert!(ensure_trade_prices_supported(&trade_prices).is_ok());
    }

    #[test]
    fn test_ensure_trade_prices_supported_rejects_xch_with_fee_policy() {
        let mut trade_price = FeeTradePrice::xch(200);
        trade_price.quote_fee_policy = Some(FeeTradePriceFeePolicy::default());
        let trade_prices = vec![trade_price];

        let result = ensure_trade_prices_supported(&trade_prices);
        assert!(matches!(result, Err(DriverError::InvalidTradePriceProfile)));
    }

    #[test]
    fn test_ensure_trade_prices_supported_rejects_invalid_xch_quote_descriptor() {
        let mut trade_price = FeeTradePrice::xch(200);
        trade_price.quote_hidden_puzzle_hash = Some(Bytes32::new([9; 32]));
        let trade_prices = vec![trade_price];

        let result = ensure_trade_prices_supported(&trade_prices);
        assert!(matches!(result, Err(DriverError::InvalidTradePriceProfile)));
    }
}
