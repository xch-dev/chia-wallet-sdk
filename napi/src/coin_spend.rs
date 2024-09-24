use chia::protocol;
use napi::bindgen_prelude::*;

use crate::{traits::IntoJs, Coin};

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
