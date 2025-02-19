mod key_pairs;

pub use key_pairs::*;

use wasm_bindgen::prelude::*;

use crate::{
    bls::{PublicKey, SecretKey},
    coin::{Coin, CoinSpend},
    traits::IntoRust,
};

#[wasm_bindgen]
#[derive(Default)]
pub struct Simulator(chia_sdk_bindings::Simulator);

#[wasm_bindgen]
impl Simulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = "newCoin")]
    pub fn new_coin(
        &mut self,
        #[wasm_bindgen(js_name = "puzzleHash")] puzzle_hash: Vec<u8>,
        amount: u64,
    ) -> Result<Coin, JsError> {
        let coin = self.0.new_coin(puzzle_hash.rust()?, amount);
        Ok(Coin {
            parent_coin_info: coin.parent_coin_info.into(),
            puzzle_hash: coin.puzzle_hash.into(),
            amount: coin.amount,
        })
    }

    pub fn bls(&mut self, amount: u64) -> BlsPairWithCoin {
        let pair = self.0.bls(amount);
        BlsPairWithCoin {
            sk: SecretKey(chia_sdk_bindings::SecretKey(pair.sk)),
            pk: PublicKey(chia_sdk_bindings::PublicKey(pair.pk)),
            puzzle_hash: pair.puzzle_hash.into(),
            coin: Coin {
                parent_coin_info: pair.coin.parent_coin_info.into(),
                puzzle_hash: pair.coin.puzzle_hash.into(),
                amount: pair.coin.amount,
            },
        }
    }

    #[wasm_bindgen(js_name = "spendCoins")]
    pub fn spend_coins(
        &mut self,
        #[wasm_bindgen(js_name = "coinSpends")] coin_spends: Vec<CoinSpend>,
        #[wasm_bindgen(js_name = "secretKeys")] secret_keys: Vec<SecretKey>,
    ) -> Result<(), JsError> {
        self.0
            .spend_coins(
                coin_spends
                    .into_iter()
                    .map(IntoRust::rust)
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                &secret_keys
                    .into_iter()
                    .map(|sk| sk.0 .0.clone())
                    .collect::<Vec<_>>(),
            )
            .map_err(chia_sdk_bindings::Error::from)?;
        Ok(())
    }
}
