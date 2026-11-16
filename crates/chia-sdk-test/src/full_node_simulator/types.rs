use chia_protocol::{BlockRecord, Bytes32, Coin, CoinSpend, CoinState, SpendBundle};
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

/// The result of [`FullNodeSimulator::push_tx_detailed`](crate::FullNodeSimulator::push_tx_detailed).
#[derive(Debug)]
pub struct FullNodeSimulatorPushTxResponse {
    /// The RPC response. Its status is `SUCCESS` or `FAILED`, and on failure its error is the
    /// display text of [`FullNodeSimulatorPushTxResponse::error`].
    pub response: PushTxResponse,
    /// The error that caused the transaction to be rejected, or [`None`] if it was accepted.
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
/// A chain event recorded by the [`FullNodeSimulator`](crate::FullNodeSimulator).
///
/// Events are collected until they are taken with
/// [`FullNodeSimulator::drain_events`](crate::FullNodeSimulator::drain_events).
pub enum FullNodeSimulatorEvent {
    /// A block was farmed.
    Block {
        /// The height of the block.
        height: u32,
        /// The header hash of the block.
        header_hash: Bytes32,
        /// The header hash of the block before it.
        previous_header_hash: Bytes32,
        /// The coins created in the block, including the reward coin.
        additions: Vec<CoinRecord>,
        /// The coins spent in the block.
        removals: Vec<CoinRecord>,
    },
    /// Blocks were replaced by [`FullNodeSimulator::reorg_blocks`](crate::FullNodeSimulator::reorg_blocks).
    ///
    /// This is recorded after the [`FullNodeSimulatorEvent::Block`] events for the new blocks.
    Reorg {
        /// The height of the last block kept from the old chain.
        fork_height: u32,
        /// The header hash of the tip before the reorg.
        old_peak_hash: Bytes32,
        /// The header hash of the tip after the reorg.
        new_peak_hash: Bytes32,
        /// The header hashes of the removed blocks, from lowest to highest.
        reverted_header_hashes: Vec<Bytes32>,
        /// The header hashes of the new blocks, from lowest to highest.
        new_header_hashes: Vec<Bytes32>,
    },
}

impl SimCoinRecord {
    #[cfg_attr(not(feature = "peer-simulator"), allow(dead_code))]
    pub(super) fn to_coin_state(self) -> CoinState {
        CoinState::new(
            self.coin,
            self.spent_block_index,
            Some(self.confirmed_block_index),
        )
    }

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
