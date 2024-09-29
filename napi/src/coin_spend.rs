use chia::protocol;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoJs, IntoRust},
    Coin,
};

#[napi(object)]
pub struct CoinSpend {
    pub coin: Coin,
    pub puzzle_reveal: Uint8Array,
    pub solution: Uint8Array,
}

impl IntoJs<CoinSpend> for protocol::CoinSpend {
    fn into_js(self) -> Result<CoinSpend> {
        Ok(CoinSpend {
            coin: self.coin.into_js()?,
            puzzle_reveal: self.puzzle_reveal.into_js()?,
            solution: self.solution.into_js()?,
        })
    }
}

impl FromJs<CoinSpend> for protocol::CoinSpend {
    fn from_js(coin_spend: CoinSpend) -> Result<Self> {
        Ok(protocol::CoinSpend {
            coin: coin_spend.coin.into_rust()?,
            puzzle_reveal: coin_spend.puzzle_reveal.into_rust()?,
            solution: coin_spend.solution.into_rust()?,
        })
    }
}
