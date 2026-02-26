use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::CoinProof;
use chia_sdk_types::puzzles::FeeTradePrice;

use crate::Spend;

#[derive(Debug, Clone)]
pub struct SingleCatSpend {
    pub prev_coin_id: Bytes32,
    pub next_coin_proof: CoinProof,
    pub prev_subtotal: i64,
    pub extra_delta: i64,
    pub p2_spend: Spend,
    pub revoke: bool,
    pub trade_nonce: Bytes32,
    pub trade_prices: Vec<FeeTradePrice>,
}

impl SingleCatSpend {
    pub fn eve(coin: Coin, inner_puzzle_hash: Bytes32, p2_spend: Spend) -> Self {
        Self {
            prev_coin_id: coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: coin.parent_coin_info,
                inner_puzzle_hash,
                amount: coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: 0,
            p2_spend,
            revoke: false,
            trade_nonce: Bytes32::default(),
            trade_prices: Vec::new(),
        }
    }
}
