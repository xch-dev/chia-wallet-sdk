use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::SecretKey;
use chia_protocol::{BlockRecord, Bytes32, Coin, SpendBundle};
use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainStateResponse, CoinRecord, GetBlockRecordResponse,
    GetBlockRecordsResponse, GetBlockSpendsResponse, GetCoinRecordResponse, GetCoinRecordsResponse,
    GetMempoolItemResponse, GetMempoolItemsResponse, GetNetworkInfoResponse,
    GetPuzzleAndSolutionResponse, PushTxResponse,
};

#[derive(Clone, Default)]
pub struct FullNodeSimulator(Arc<Mutex<chia_sdk_test::FullNodeSimulator>>);

#[derive(Debug, Clone)]
pub struct FullNodeSimulatorEvent(chia_sdk_test::FullNodeSimulatorEvent);

#[derive(Debug, Clone)]
pub struct FullNodeSimulatorBlockEvent {
    pub height: u32,
    pub header_hash: Bytes32,
    pub previous_header_hash: Bytes32,
    pub additions: Vec<CoinRecord>,
    pub removals: Vec<CoinRecord>,
}

#[derive(Debug, Clone)]
pub struct FullNodeSimulatorReorgEvent {
    pub fork_height: u32,
    pub old_peak_hash: Bytes32,
    pub new_peak_hash: Bytes32,
    pub reverted_header_hashes: Vec<Bytes32>,
    pub new_header_hashes: Vec<Bytes32>,
}

#[cfg(any(feature = "napi", feature = "pyo3"))]
#[derive(Clone)]
pub struct FullNodeSimulatorServer {
    pub url: String,
    server: Arc<Mutex<Option<chia_sdk_test::FullNodeSimulatorServer>>>,
}

impl FullNodeSimulator {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn with_seed(seed: u64) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            chia_sdk_test::FullNodeSimulator::with_seed(seed),
        ))))
    }

    pub fn with_secret_key(secret_key: SecretKey) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            chia_sdk_test::FullNodeSimulator::with_secret_key(secret_key),
        ))))
    }

    pub fn height(&self) -> Result<u32> {
        Ok(self.0.lock().unwrap().height())
    }

    pub fn header_hash(&self) -> Result<Bytes32> {
        Ok(self.0.lock().unwrap().header_hash())
    }

    pub fn header_hash_of(&self, height: u32) -> Result<Option<Bytes32>> {
        Ok(self.0.lock().unwrap().header_hash_of(height))
    }

    pub fn insert_coin(&self, coin: Coin) -> Result<()> {
        self.0.lock().unwrap().insert_coin(coin);
        Ok(())
    }

    pub fn new_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Result<Coin> {
        Ok(self.0.lock().unwrap().new_coin(puzzle_hash, amount))
    }

    pub fn get_farming_ph(&self) -> Result<Bytes32> {
        Ok(self.0.lock().unwrap().get_farming_ph())
    }

    pub fn get_master_secret_key(&self) -> Result<SecretKey> {
        Ok(self.0.lock().unwrap().get_master_secret_key())
    }

    pub fn get_prefarm_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.0.lock().unwrap().get_prefarm_puzzle_hash())
    }

    pub fn set_farming_ph(&self, puzzle_hash: Bytes32) -> Result<()> {
        self.0.lock().unwrap().set_farming_ph(puzzle_hash);
        Ok(())
    }

    pub fn get_blockchain_state(&self) -> Result<BlockchainStateResponse> {
        Ok(self.0.lock().unwrap().get_blockchain_state())
    }

    pub fn get_network_info(&self) -> Result<GetNetworkInfoResponse> {
        Ok(self.0.lock().unwrap().get_network_info())
    }

    pub fn get_aggsig_additional_data(&self) -> Result<Bytes32> {
        Ok(self.0.lock().unwrap().get_aggsig_additional_data())
    }

    pub fn get_block_record(&self, header_hash: Bytes32) -> Result<GetBlockRecordResponse> {
        Ok(self.0.lock().unwrap().get_block_record(header_hash))
    }

    pub fn get_block_record_by_height(&self, height: u32) -> Result<GetBlockRecordResponse> {
        Ok(self.0.lock().unwrap().get_block_record_by_height(height))
    }

    pub fn get_block_records(&self, start: u32, end: u32) -> Result<GetBlockRecordsResponse> {
        Ok(self.0.lock().unwrap().get_block_records(start, end))
    }

    pub fn get_additions_and_removals(
        &self,
        header_hash: Bytes32,
    ) -> Result<AdditionsAndRemovalsResponse> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get_additions_and_removals(header_hash))
    }

    pub fn get_block_spends(&self, header_hash: Bytes32) -> Result<GetBlockSpendsResponse> {
        Ok(self.0.lock().unwrap().get_block_spends(header_hash))
    }

    pub fn get_coin_record_by_name(&self, name: Bytes32) -> Result<GetCoinRecordResponse> {
        Ok(self.0.lock().unwrap().get_coin_record_by_name(name))
    }

    pub fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_names(
            &names,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_hint(
            hint,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_coin_records_by_hints(
        &self,
        hints: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_hints(
            hints,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_parent_ids(
            parent_ids,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_puzzle_hash(
            puzzle_hash,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(self.0.lock().unwrap().get_coin_records_by_puzzle_hashes(
            puzzle_hashes,
            start_height,
            end_height,
            include_spent_coins,
        ))
    }

    pub fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> Result<GetPuzzleAndSolutionResponse> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get_puzzle_and_solution(coin_id, height))
    }

    pub fn push_tx(&self, spend_bundle: SpendBundle) -> Result<PushTxResponse> {
        Ok(self.0.lock().unwrap().push_tx(spend_bundle))
    }

    pub fn get_mempool_item_by_tx_id(&self, tx_id: Bytes32) -> Result<GetMempoolItemResponse> {
        Ok(self.0.lock().unwrap().get_mempool_item_by_tx_id(tx_id))
    }

    pub fn get_mempool_items_by_coin_name(
        &self,
        coin_name: Bytes32,
    ) -> Result<GetMempoolItemsResponse> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get_mempool_items_by_coin_name(coin_name))
    }

    pub fn farm_block(&self, blocks: u32) -> Result<Vec<BlockRecord>> {
        Ok(self.0.lock().unwrap().farm_block(blocks))
    }

    pub fn revert_blocks(&self, blocks: u32) -> Result<Vec<Bytes32>> {
        Ok(self.0.lock().unwrap().revert_blocks(blocks))
    }

    pub fn reorg_blocks(
        &self,
        num_of_blocks_to_rev: u32,
        num_of_new_blocks: u32,
    ) -> Result<Vec<BlockRecord>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .reorg_blocks(num_of_blocks_to_rev, num_of_new_blocks))
    }

    pub fn drain_events(&self) -> Result<Vec<FullNodeSimulatorEvent>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .drain_events()
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub fn dump_state(&self) -> Result<String> {
        self.0
            .lock()
            .unwrap()
            .dump_state()
            .map_err(|error| bindy::Error::Custom(error.to_string()))
    }

    pub fn restore_state(&self, state: String) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .restore_state(&state)
            .map_err(|error| bindy::Error::Custom(error.to_string()))
    }

    #[cfg(any(feature = "napi", feature = "pyo3"))]
    pub async fn start_server(&self) -> Result<FullNodeSimulatorServer> {
        let server = chia_sdk_test::FullNodeSimulatorServer::with_simulator(self.0.clone())
            .await
            .map_err(|error| bindy::Error::Custom(error.to_string()))?;
        Ok(FullNodeSimulatorServer {
            url: server.url(),
            server: Arc::new(Mutex::new(Some(server))),
        })
    }
}

impl From<chia_sdk_test::FullNodeSimulatorEvent> for FullNodeSimulatorEvent {
    fn from(event: chia_sdk_test::FullNodeSimulatorEvent) -> Self {
        Self(event)
    }
}

impl FullNodeSimulatorEvent {
    pub fn block(&self) -> Result<Option<FullNodeSimulatorBlockEvent>> {
        Ok(match &self.0 {
            chia_sdk_test::FullNodeSimulatorEvent::Block {
                height,
                header_hash,
                previous_header_hash,
                additions,
                removals,
            } => Some(FullNodeSimulatorBlockEvent {
                height: *height,
                header_hash: *header_hash,
                previous_header_hash: *previous_header_hash,
                additions: additions.clone(),
                removals: removals.clone(),
            }),
            chia_sdk_test::FullNodeSimulatorEvent::Reorg { .. } => None,
        })
    }

    pub fn reorg(&self) -> Result<Option<FullNodeSimulatorReorgEvent>> {
        Ok(match &self.0 {
            chia_sdk_test::FullNodeSimulatorEvent::Reorg {
                fork_height,
                old_peak_hash,
                new_peak_hash,
                reverted_header_hashes,
                new_header_hashes,
            } => Some(FullNodeSimulatorReorgEvent {
                fork_height: *fork_height,
                old_peak_hash: *old_peak_hash,
                new_peak_hash: *new_peak_hash,
                reverted_header_hashes: reverted_header_hashes.clone(),
                new_header_hashes: new_header_hashes.clone(),
            }),
            chia_sdk_test::FullNodeSimulatorEvent::Block { .. } => None,
        })
    }
}

#[cfg(any(feature = "napi", feature = "pyo3"))]
impl FullNodeSimulatorServer {
    pub fn close(&self) -> Result<()> {
        self.server.lock().unwrap().take();
        Ok(())
    }
}
