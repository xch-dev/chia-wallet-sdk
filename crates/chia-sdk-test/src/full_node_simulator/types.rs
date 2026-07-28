use chia_protocol::{BlockRecord, Bytes32, Coin, CoinSpend, SpendBundle};
use chia_sdk_coinset::{CoinRecord, MempoolItem, PushTxResponse};
use indexmap::IndexMap;

use crate::SimulatorError;

use super::state::BlockDelta;

#[derive(Debug, Clone)]
pub(super) struct SimBlock {
    pub(super) record: BlockRecord,
    pub(super) additions: Vec<Bytes32>,
    pub(super) removals: Vec<Bytes32>,
    pub(super) spends: Vec<CoinSpend>,
    pub(super) transactions: Vec<SpendBundle>,
    pub(super) delta: BlockDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct SimCoinRecord {
    pub(super) coin: Coin,
    pub(super) coinbase: bool,
    pub(super) confirmed_block_index: u32,
    pub(super) spent_block_index: Option<u32>,
    pub(super) timestamp: u64,
}

#[derive(Debug)]
pub struct FullNodeSimulatorPushTxResponse {
    pub response: PushTxResponse,
    pub error: Option<SimulatorError>,
}

#[derive(Debug, Clone)]
pub(super) struct ValidatedBundle {
    pub(super) spend_bundle: SpendBundle,
    pub(super) removals: Vec<Bytes32>,
    pub(super) additions: Vec<(Coin, Option<Bytes32>)>,
    pub(super) spends: IndexMap<Bytes32, ValidatedSpend>,
    pub(super) cost: u64,
    pub(super) fee: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ValidatedSpend {
    pub(super) coin_spend: CoinSpend,
    pub(super) flags: u32,
    pub(super) fingerprint: Option<Bytes32>,
    pub(super) additions: Vec<(Coin, Option<Bytes32>)>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FullNodeSimulatorEvent {
    Block {
        height: u32,
        header_hash: Bytes32,
        previous_header_hash: Bytes32,
        additions: Vec<CoinRecord>,
        removals: Vec<CoinRecord>,
    },
    Reorg {
        fork_height: u32,
        old_peak_hash: Bytes32,
        new_peak_hash: Bytes32,
        reverted_header_hashes: Vec<Bytes32>,
        new_header_hashes: Vec<Bytes32>,
    },
}

impl SimCoinRecord {
    pub(super) fn to_coin_record(self) -> CoinRecord {
        CoinRecord {
            coin: self.coin,
            coinbase: self.coinbase,
            confirmed_block_index: self.confirmed_block_index,
            spent: self.spent_block_index.is_some(),
            spent_block_index: self.spent_block_index.unwrap_or(0),
            timestamp: self.timestamp,
        }
    }
}

impl ValidatedBundle {
    pub(super) fn to_mempool_item(&self) -> MempoolItem {
        MempoolItem {
            spend_bundle: self.spend_bundle.clone(),
            fee: self.fee,
        }
    }
}
