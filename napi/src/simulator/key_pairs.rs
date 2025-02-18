use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{
    Coin, IntoRust, K1PublicKey, K1SecretKey, PublicKey, R1PublicKey, R1SecretKey, SecretKey,
};

#[napi]
pub struct BlsPair {
    sk: Reference<SecretKey>,
    pk: Reference<PublicKey>,
}

#[napi]
impl BlsPair {
    #[napi(constructor)]
    pub fn new(env: Env, seed: BigInt) -> Result<Self> {
        let pair = chia_sdk_bindings::BlsPair::new(seed.rust()?);
        Ok(Self {
            sk: SecretKey(chia_sdk_bindings::SecretKey(pair.sk)).into_reference(env)?,
            pk: PublicKey(chia_sdk_bindings::PublicKey(pair.pk)).into_reference(env)?,
        })
    }

    #[napi(getter)]
    pub fn sk(&self, env: Env) -> Result<Reference<SecretKey>> {
        self.sk.clone(env)
    }

    #[napi(getter)]
    pub fn pk(&self, env: Env) -> Result<Reference<PublicKey>> {
        self.pk.clone(env)
    }
}

#[napi]
pub struct BlsPairWithCoin {
    pub(crate) sk: Reference<SecretKey>,
    pub(crate) pk: Reference<PublicKey>,
    pub(crate) puzzle_hash: Uint8Array,
    pub(crate) coin: Coin,
}

#[napi]
impl BlsPairWithCoin {
    #[napi(getter)]
    pub fn sk(&self, env: Env) -> Result<Reference<SecretKey>> {
        self.sk.clone(env)
    }

    #[napi(getter)]
    pub fn pk(&self, env: Env) -> Result<Reference<PublicKey>> {
        self.pk.clone(env)
    }

    #[napi(getter)]
    pub fn puzzle_hash(&self) -> Uint8Array {
        self.puzzle_hash.to_vec().into()
    }

    #[napi(getter)]
    pub fn coin(&self) -> Coin {
        Coin {
            parent_coin_info: self.coin.parent_coin_info.to_vec().into(),
            puzzle_hash: self.coin.puzzle_hash.to_vec().into(),
            amount: self.coin.amount.clone(),
        }
    }
}

#[napi]
pub struct K1Pair {
    sk: Reference<K1SecretKey>,
    pk: Reference<K1PublicKey>,
}

#[napi]
impl K1Pair {
    #[napi(constructor)]
    pub fn new(env: Env, seed: BigInt) -> Result<Self> {
        let pair = chia_sdk_bindings::K1Pair::new(seed.rust()?);
        Ok(Self {
            sk: K1SecretKey(chia_sdk_bindings::K1SecretKey(pair.sk)).into_reference(env)?,
            pk: K1PublicKey(chia_sdk_bindings::K1PublicKey(pair.pk)).into_reference(env)?,
        })
    }

    #[napi(getter)]
    pub fn sk(&self, env: Env) -> Result<Reference<K1SecretKey>> {
        self.sk.clone(env)
    }

    #[napi(getter)]
    pub fn pk(&self, env: Env) -> Result<Reference<K1PublicKey>> {
        self.pk.clone(env)
    }
}

#[napi]
pub struct R1Pair {
    sk: Reference<R1SecretKey>,
    pk: Reference<R1PublicKey>,
}

#[napi]
impl R1Pair {
    #[napi(constructor)]
    pub fn new(env: Env, seed: BigInt) -> Result<Self> {
        let pair = chia_sdk_bindings::R1Pair::new(seed.rust()?);
        Ok(Self {
            sk: R1SecretKey(chia_sdk_bindings::R1SecretKey(pair.sk)).into_reference(env)?,
            pk: R1PublicKey(chia_sdk_bindings::R1PublicKey(pair.pk)).into_reference(env)?,
        })
    }

    #[napi(getter)]
    pub fn sk(&self, env: Env) -> Result<Reference<R1SecretKey>> {
        self.sk.clone(env)
    }

    #[napi(getter)]
    pub fn pk(&self, env: Env) -> Result<Reference<R1PublicKey>> {
        self.pk.clone(env)
    }
}
