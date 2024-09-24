use napi::bindgen_prelude::*;

use crate::Coin;

#[napi(object)]
pub struct CoinSpend {
    pub coin: Coin,
    pub puzzle_reveal: Uint8Array,
    pub solution: Uint8Array,
}
