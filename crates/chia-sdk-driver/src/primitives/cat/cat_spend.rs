use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::FeeTradePrice;

use crate::Spend;

use super::Cat;

#[derive(Debug, Clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
    pub hidden: bool,
    pub trade_nonce: Bytes32,
    pub trade_prices: Vec<FeeTradePrice>,
}

impl CatSpend {
    pub fn new(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: false,
            trade_nonce: Bytes32::default(),
            trade_prices: Vec::new(),
        }
    }

    pub fn revoke(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: true,
            trade_nonce: Bytes32::default(),
            trade_prices: Vec::new(),
        }
    }

    pub fn with_trade(mut self, trade_nonce: Bytes32, trade_prices: Vec<FeeTradePrice>) -> Self {
        self.trade_nonce = trade_nonce;
        self.trade_prices = trade_prices;
        self
    }
}
