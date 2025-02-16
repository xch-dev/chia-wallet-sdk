use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Coin {
    #[wasm_bindgen(js_name = "parentCoinInfo")]
    pub parent_coin_info: Vec<u8>,
    #[wasm_bindgen(js_name = "puzzleHash")]
    pub puzzle_hash: Vec<u8>,
    pub amount: u64,
}

#[wasm_bindgen(getter_with_clone)]
pub struct CoinState {
    pub coin: Coin,
    #[wasm_bindgen(js_name = "spentHeight")]
    pub spent_height: Option<u32>,
    #[wasm_bindgen(js_name = "createdHeight")]
    pub created_height: Option<u32>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct CoinSpend {
    pub coin: Coin,
    #[wasm_bindgen(js_name = "puzzleReveal")]
    pub puzzle_reveal: Vec<u8>,
    pub solution: Vec<u8>,
}
