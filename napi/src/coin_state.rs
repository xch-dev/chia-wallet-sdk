use chia::protocol;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoJs, IntoRust},
    Coin,
};

#[napi(object)]
pub struct CoinState {
    pub coin: Coin,
    pub spent_height: Option<u32>,
    pub created_height: Option<u32>,
}

impl IntoJs<CoinState> for protocol::CoinState {
    fn into_js(self) -> Result<CoinState> {
        Ok(CoinState {
            coin: self.coin.into_js()?,
            spent_height: self.spent_height,
            created_height: self.created_height,
        })
    }
}

impl FromJs<CoinState> for protocol::CoinState {
    fn from_js(value: CoinState) -> Result<Self> {
        Ok(Self {
            coin: value.coin.into_rust()?,
            spent_height: value.spent_height,
            created_height: value.created_height,
        })
    }
}
