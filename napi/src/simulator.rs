mod key_pairs;

pub use key_pairs::*;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{Coin, CoinSpend, IntoRust, PublicKey, SecretKey};

#[napi]
#[derive(Default)]
pub struct Simulator(chia_sdk_bindings::Simulator);

#[napi]
impl Simulator {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi]
    pub fn new_coin(&mut self, puzzle_hash: Uint8Array, amount: BigInt) -> Result<Coin> {
        let coin = self.0.new_coin(puzzle_hash.rust()?, amount.rust()?);
        Ok(Coin {
            parent_coin_info: coin.parent_coin_info.into(),
            puzzle_hash: coin.puzzle_hash.into(),
            amount: coin.amount.into(),
        })
    }

    #[napi]
    pub fn bls(&mut self, env: Env, amount: BigInt) -> Result<BlsPairWithCoin> {
        let pair = self.0.bls(amount.rust()?);
        Ok(BlsPairWithCoin {
            sk: SecretKey(chia_sdk_bindings::SecretKey(pair.sk)).into_reference(env)?,
            pk: PublicKey(chia_sdk_bindings::PublicKey(pair.pk)).into_reference(env)?,
            puzzle_hash: pair.puzzle_hash.into(),
            coin: Coin {
                parent_coin_info: pair.coin.parent_coin_info.into(),
                puzzle_hash: pair.coin.puzzle_hash.into(),
                amount: pair.coin.amount.into(),
            },
        })
    }

    #[napi]
    pub fn spend_coins(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: Vec<ClassInstance<'_, SecretKey>>,
    ) -> Result<()> {
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
