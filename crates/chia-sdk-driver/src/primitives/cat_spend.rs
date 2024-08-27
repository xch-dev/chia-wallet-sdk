use chia_protocol::{Bytes32, Coin};
use chia_puzzles::CoinProof;

use crate::Spend;

use super::Cat;

#[derive(Debug, Clone, Copy)]
pub struct RawCatSpend {
    pub prev_coin_id: Bytes32,
    pub next_coin_proof: CoinProof,
    pub prev_subtotal: i64,
    pub extra_delta: i64,
    pub inner_spend: Spend,
}

impl RawCatSpend {
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
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CatSpend {
    pub cat: Cat,
    pub inner_spend: Spend,
    pub extra_delta: i64,
}

impl CatSpend {
    pub fn new(cat: Cat, inner_spend: Spend) -> Self {
        Self {
            cat,
            inner_spend,
            extra_delta: 0,
        }
    }

    pub fn with_extra_delta(cat: Cat, inner_spend: Spend, extra_delta: i64) -> Self {
        Self {
            cat,
            inner_spend,
            extra_delta,
        }
    }
}
