use pyo3::prelude::*;

use crate::{
    bls::{PublicKey, SecretKey},
    coin::Coin,
    secp::{K1PublicKey, K1SecretKey, R1PublicKey, R1SecretKey},
};

#[pyclass(get_all, frozen)]
pub struct BlsPair {
    pub sk: SecretKey,
    pub pk: PublicKey,
}

#[pymethods]
impl BlsPair {
    #[new]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::BlsPair::new(seed);
        Self {
            sk: SecretKey(chia_sdk_bindings::SecretKey(pair.sk)),
            pk: PublicKey(chia_sdk_bindings::PublicKey(pair.pk)),
        }
    }
}

#[pyclass(get_all, frozen)]
pub struct BlsPairWithCoin {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Vec<u8>,
    pub coin: Coin,
}

#[pyclass(get_all, frozen)]
pub struct K1Pair {
    pub sk: K1SecretKey,
    pub pk: K1PublicKey,
}

#[pymethods]
impl K1Pair {
    #[new]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::K1Pair::new(seed);
        Self {
            sk: K1SecretKey(chia_sdk_bindings::K1SecretKey(pair.sk)),
            pk: K1PublicKey(chia_sdk_bindings::K1PublicKey(pair.pk)),
        }
    }
}

#[pyclass(get_all, frozen)]
pub struct R1Pair {
    pub sk: R1SecretKey,
    pub pk: R1PublicKey,
}

#[pymethods]
impl R1Pair {
    #[new]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::R1Pair::new(seed);
        Self {
            sk: R1SecretKey(chia_sdk_bindings::R1SecretKey(pair.sk)),
            pk: R1PublicKey(chia_sdk_bindings::R1PublicKey(pair.pk)),
        }
    }
}
