use wasm_bindgen::prelude::*;

use crate::{
    bls::{PublicKey, SecretKey},
    coin::Coin,
    secp::{K1PublicKey, K1SecretKey, R1PublicKey, R1SecretKey},
};

#[wasm_bindgen]
pub struct BlsPair {
    pub(crate) sk: SecretKey,
    pub(crate) pk: PublicKey,
}

#[wasm_bindgen]
impl BlsPair {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::BlsPair::new(seed);
        Self {
            sk: SecretKey(chia_sdk_bindings::SecretKey(pair.sk)),
            pk: PublicKey(chia_sdk_bindings::PublicKey(pair.pk)),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sk(&self) -> SecretKey {
        self.sk.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pk(&self) -> PublicKey {
        self.pk.clone()
    }
}

#[wasm_bindgen]
pub struct BlsPairWithCoin {
    pub(crate) sk: SecretKey,
    pub(crate) pk: PublicKey,
    pub(crate) puzzle_hash: Vec<u8>,
    pub(crate) coin: Coin,
}

#[wasm_bindgen]
impl BlsPairWithCoin {
    #[wasm_bindgen(getter)]
    pub fn sk(&self) -> SecretKey {
        self.sk.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pk(&self) -> PublicKey {
        self.pk.clone()
    }

    #[wasm_bindgen(getter, js_name = "puzzleHash")]
    pub fn puzzle_hash(&self) -> Vec<u8> {
        self.puzzle_hash.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn coin(&self) -> Coin {
        self.coin.clone()
    }
}

#[wasm_bindgen]
pub struct K1Pair {
    pub(crate) sk: K1SecretKey,
    pub(crate) pk: K1PublicKey,
}

#[wasm_bindgen]
impl K1Pair {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::K1Pair::new(seed);
        Self {
            sk: K1SecretKey(chia_sdk_bindings::K1SecretKey(pair.sk)),
            pk: K1PublicKey(chia_sdk_bindings::K1PublicKey(pair.pk)),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sk(&self) -> K1SecretKey {
        self.sk.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pk(&self) -> K1PublicKey {
        self.pk.clone()
    }
}

#[wasm_bindgen]
pub struct R1Pair {
    pub(crate) sk: R1SecretKey,
    pub(crate) pk: R1PublicKey,
}

#[wasm_bindgen]
impl R1Pair {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64) -> Self {
        let pair = chia_sdk_bindings::R1Pair::new(seed);
        Self {
            sk: R1SecretKey(chia_sdk_bindings::R1SecretKey(pair.sk)),
            pk: R1PublicKey(chia_sdk_bindings::R1PublicKey(pair.pk)),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn sk(&self) -> R1SecretKey {
        self.sk.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pk(&self) -> R1PublicKey {
        self.pk.clone()
    }
}
