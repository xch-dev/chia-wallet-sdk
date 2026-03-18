use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::TransferFeeTradePrice;

use crate::Spend;

use super::Cat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatTransferFeeContext {
    pub trade_nonce: Bytes32,
    pub trade_prices: Vec<TransferFeeTradePrice>,
}

impl CatTransferFeeContext {
    pub fn new(trade_nonce: Bytes32, trade_prices: Vec<TransferFeeTradePrice>) -> Self {
        Self {
            trade_nonce,
            trade_prices,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
    pub hidden: bool,
    pub transfer_fee_context: Option<CatTransferFeeContext>,
}

impl CatSpend {
    pub fn new(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: false,
            transfer_fee_context: None,
        }
    }

    pub fn revoke(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: true,
            transfer_fee_context: None,
        }
    }

    pub fn with_transfer_fee_context(
        mut self,
        trade_nonce: Bytes32,
        trade_prices: Vec<TransferFeeTradePrice>,
    ) -> Self {
        self.transfer_fee_context = Some(CatTransferFeeContext::new(trade_nonce, trade_prices));
        self
    }
}
