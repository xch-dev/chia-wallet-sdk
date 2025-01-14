use chia::secp;
use chia_wallet_sdk as sdk;
use napi::bindgen_prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::{
    traits::{js_err, FromJs, FromRust, IntoJs, IntoRust},
    Coin, CoinSpend, K1PublicKey, K1SecretKey, PublicKey, R1PublicKey, R1SecretKey, SecretKey,
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
    pub fn new_p2(&mut self, env: Env, amount: BigInt) -> Result<P2Coin> {
        let (secret_key, public_key, puzzle_hash, coin) =
            self.0.new_p2(amount.into_rust()?).map_err(js_err)?;

        Ok(P2Coin {
            coin: coin.into_js()?,
            puzzle_hash: puzzle_hash.into_js()?,
            public_key: PublicKey::from_rust(public_key)?.into_instance(env)?,
            secret_key: SecretKey::from_rust(secret_key)?.into_instance(env)?,
        })
    }

    #[napi]
    pub fn spend(
        &mut self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: Vec<Reference<SecretKey>>,
    ) -> Result<()> {
        self.0
            .spend_coins(
                coin_spends
                    .into_iter()
                    .map(FromJs::from_js)
                    .collect::<Result<Vec<_>>>()?,
                &secret_keys
                    .into_iter()
                    .map(|sk| sk.0.clone())
                    .collect::<Vec<_>>(),
            )
            .map_err(js_err)?;
        Ok(())
    }

    #[napi]
    pub fn k1_pair(&mut self, env: Env, seed: u32) -> Result<K1KeyPair> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed.into());

        let secret_key = secp::K1SecretKey::from_bytes(&rng.gen()).map_err(js_err)?;
        let public_key = secret_key.public_key();

        Ok(K1KeyPair {
            public_key: K1PublicKey::from_rust(public_key)?.into_instance(env)?,
            secret_key: K1SecretKey::from_rust(secret_key)?.into_instance(env)?,
        })
    }

    #[napi]
    pub fn r1_pair(&mut self, env: Env, seed: u32) -> Result<R1KeyPair> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed.into());

        let secret_key = secp::R1SecretKey::from_bytes(&rng.gen()).map_err(js_err)?;
        let public_key = secret_key.public_key();

        Ok(R1KeyPair {
            public_key: R1PublicKey::from_rust(public_key)?.into_instance(env)?,
            secret_key: R1SecretKey::from_rust(secret_key)?.into_instance(env)?,
        })
    }
}

#[napi(object)]
pub struct P2Coin {
    pub coin: Coin,
    pub puzzle_hash: Uint8Array,
    pub public_key: ClassInstance<PublicKey>,
    pub secret_key: ClassInstance<SecretKey>,
}

#[napi(object)]
pub struct K1KeyPair {
    pub public_key: ClassInstance<K1PublicKey>,
    pub secret_key: ClassInstance<K1SecretKey>,
}

#[napi(object)]
pub struct R1KeyPair {
    pub public_key: ClassInstance<R1PublicKey>,
    pub secret_key: ClassInstance<R1SecretKey>,
}
