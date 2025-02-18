use wasm_bindgen::prelude::wasm_bindgen;

use crate::Signature;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Coin {
    #[wasm_bindgen(js_name = "parentCoinInfo")]
    pub parent_coin_info: Vec<u8>,
    #[wasm_bindgen(js_name = "puzzleHash")]
    pub puzzle_hash: Vec<u8>,
    pub amount: u64,
}

#[wasm_bindgen]
impl Coin {
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(js_name = "parentCoinInfo")] parent_coin_info: Vec<u8>,
        #[wasm_bindgen(js_name = "puzzleHash")] puzzle_hash: Vec<u8>,
        amount: u64,
    ) -> Self {
        Self {
            parent_coin_info,
            puzzle_hash,
            amount,
        }
    }
}

#[wasm_bindgen(getter_with_clone)]
pub struct CoinState {
    pub coin: Coin,
    #[wasm_bindgen(js_name = "spentHeight")]
    pub spent_height: Option<u32>,
    #[wasm_bindgen(js_name = "createdHeight")]
    pub created_height: Option<u32>,
}

#[wasm_bindgen]
impl CoinState {
    #[wasm_bindgen(constructor)]
    pub fn new(
        coin: Coin,
        #[wasm_bindgen(js_name = "spentHeight")] spent_height: Option<u32>,
        #[wasm_bindgen(js_name = "createdHeight")] created_height: Option<u32>,
    ) -> Self {
        Self {
            coin,
            spent_height,
            created_height,
        }
    }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct CoinSpend {
    pub coin: Coin,
    #[wasm_bindgen(js_name = "puzzleReveal")]
    pub puzzle_reveal: Vec<u8>,
    pub solution: Vec<u8>,
}

#[wasm_bindgen]
impl CoinSpend {
    #[wasm_bindgen(constructor)]
    pub fn new(
        coin: Coin,
        #[wasm_bindgen(js_name = "puzzleReveal")] puzzle_reveal: Vec<u8>,
        solution: Vec<u8>,
    ) -> Self {
        Self {
            coin,
            puzzle_reveal,
            solution,
        }
    }
}

#[wasm_bindgen(getter_with_clone)]
pub struct SpendBundle {
    #[wasm_bindgen(js_name = "coinSpends")]
    pub coin_spends: Vec<CoinSpend>,
    #[wasm_bindgen(js_name = "aggregatedSignature")]
    pub aggregated_signature: Signature,
}

#[wasm_bindgen]
impl SpendBundle {
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(js_name = "coinSpends")] coin_spends: Vec<CoinSpend>,
        #[wasm_bindgen(js_name = "aggregatedSignature")] aggregated_signature: Signature,
    ) -> Self {
        Self {
            coin_spends,
            aggregated_signature,
        }
    }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct LineageProof {
    pub parent_parent_coin_info: Vec<u8>,
    pub parent_inner_puzzle_hash: Option<Vec<u8>>,
    pub parent_amount: u64,
}

#[wasm_bindgen]
impl LineageProof {
    #[wasm_bindgen(constructor)]
    pub fn new(
        parent_parent_coin_info: Vec<u8>,
        parent_inner_puzzle_hash: Option<Vec<u8>>,
        parent_amount: u64,
    ) -> Self {
        Self {
            parent_parent_coin_info,
            parent_inner_puzzle_hash,
            parent_amount,
        }
    }
}
