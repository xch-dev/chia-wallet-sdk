use chia::protocol;
use napi::bindgen_prelude::*;

use crate::traits::{FromJs, IntoJs, IntoRust};

#[napi(object)]
#[derive(Clone)]
pub struct Coin {
    pub parent_coin_info: Uint8Array,
    pub puzzle_hash: Uint8Array,
    pub amount: BigInt,
}

impl IntoJs<Coin> for protocol::Coin {
    fn into_js(self) -> Result<Coin> {
        Ok(Coin {
            parent_coin_info: self.parent_coin_info.into_js()?,
            puzzle_hash: self.puzzle_hash.into_js()?,
            amount: self.amount.into_js()?,
        })
    }
}

impl FromJs<Coin> for protocol::Coin {
    fn from_js(value: Coin) -> Result<Self> {
        Ok(Self {
            parent_coin_info: value.parent_coin_info.into_rust()?,
            puzzle_hash: value.puzzle_hash.into_rust()?,
            amount: value.amount.into_rust()?,
        })
    }
}

#[napi]
pub fn to_coin_id(coin: Coin) -> Result<Uint8Array> {
    protocol::Coin::from_js(coin)?.coin_id().into_js()
}
