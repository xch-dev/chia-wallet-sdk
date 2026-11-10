use chia_protocol::{Coin, SpendBundle};
use serde::{Deserialize, Serialize};

/// A coin and the state it's in on the blockchain, as returned by the RPC.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoinRecord {
    /// The coin itself.
    pub coin: Coin,
    /// Whether the coin is a farmer or pool reward created by a block, rather than by a spend.
    pub coinbase: bool,
    /// The height of the block that created the coin.
    pub confirmed_block_index: u32,
    /// Whether the coin has been spent.
    pub spent: bool,
    /// The height of the block that spent the coin, or `0` if it's unspent.
    pub spent_block_index: u32,
    /// The timestamp of the block that created the coin, in seconds since the Unix epoch.
    pub timestamp: u64,
}

/// A transaction in the mempool, as returned by the RPC.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MempoolItem {
    /// The spend bundle that was submitted.
    pub spend_bundle: SpendBundle,
    /// The fee paid by the spend bundle, in mojos.
    pub fee: u64,
}
