use chia_protocol::{Coin, SpendBundle};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoinRecord {
    pub coin: Coin,
    pub coinbase: bool,
    pub confirmed_block_index: u32,
    pub spent: bool,
    pub spent_block_index: u32,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MempoolItem {
    pub spend_bundle: SpendBundle,
    pub fee: u64,
}
