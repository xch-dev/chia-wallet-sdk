use chia::bls::SecretKey;
use chia_wallet_sdk as sdk;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoJs, IntoRust},
    Coin, CoinSpend,
};

#[napi]
pub struct Simulator(sdk::Simulator);

#[napi]
impl Simulator {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self(sdk::Simulator::new())
    }

    #[napi]
    pub fn new_coin(&mut self, puzzle_hash: Uint8Array, amount: BigInt) -> Result<Coin> {
        self.0
            .new_coin(puzzle_hash.into_rust()?, amount.into_rust()?)
            .into_js()
    }

    #[napi]
    pub fn new_p2(&mut self, amount: BigInt) -> Result<P2Coin> {
        let (secret_key, public_key, puzzle_hash, coin) = self
            .0
            .new_p2(amount.into_rust()?)
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(P2Coin {
            coin: coin.into_js()?,
            puzzle_hash: puzzle_hash.into_js()?,
            public_key: public_key.to_bytes().into_js()?,
            secret_key: secret_key.to_bytes().into_js()?,
        })
    }

    #[napi]
    pub fn spend(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: Vec<Uint8Array>,
    ) -> Result<()> {
        self.0
            .spend_coins(
                coin_spends
                    .into_iter()
                    .map(FromJs::from_js)
                    .collect::<Result<Vec<_>>>()?,
                &secret_keys
                    .into_iter()
                    .map(|sk| {
                        SecretKey::from_bytes(&sk.into_rust()?)
                            .map_err(|error| Error::from_reason(error.to_string()))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(())
    }
}

#[napi(object)]
pub struct P2Coin {
    pub coin: Coin,
    pub puzzle_hash: Uint8Array,
    pub public_key: Uint8Array,
    pub secret_key: Uint8Array,
}
