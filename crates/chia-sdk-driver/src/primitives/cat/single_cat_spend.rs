use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::CoinProof;

use crate::Spend;

#[derive(Debug, Clone, Copy)]
pub struct SingleCatSpend {
    pub prev_coin_id: Bytes32,
    pub next_coin_proof: CoinProof,
    pub prev_subtotal: i64,
    pub extra_delta: i64,
    pub inner_spend: Spend,
    pub revoke: bool,
}

impl SingleCatSpend {
    pub fn eve(coin: Coin, inner_puzzle_hash: Bytes32, inner_spend: Spend) -> Self {
        Self {
            prev_coin_id: coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: coin.parent_coin_info,
                inner_puzzle_hash,
                amount: coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: 0,
            inner_spend,
            revoke: false,
        }
    }
}
