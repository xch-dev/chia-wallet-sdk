use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::Signature;

#[napi(object)]
pub struct Coin {
    pub parent_coin_info: Uint8Array,
    pub puzzle_hash: Uint8Array,
    pub amount: BigInt,
}

#[napi(object)]
pub struct CoinState {
    pub coin: Coin,
    pub spent_height: Option<u32>,
    pub created_height: Option<u32>,
}

#[napi(object)]
pub struct CoinSpend {
    pub coin: Coin,
    pub puzzle_reveal: Uint8Array,
    pub solution: Uint8Array,
}

#[napi(object)]
pub struct SpendBundle {
    pub coin_spends: Vec<CoinSpend>,
    pub aggregated_signature: Reference<Signature>,
}
