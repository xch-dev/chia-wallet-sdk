use chia_protocol::{Bytes32, CoinState, Program, SpendBundle};
use indexmap::IndexSet;

use crate::{
    FullNodeSimulator, Simulator, SimulatorError,
    full_node_simulator::{MempoolItemIds, PeerEvent},
};

use super::PeerSimulatorConfig;

/// The operations a [`PeerSimulator`](crate::PeerSimulator) needs from the simulator it serves.
///
/// This is public only so that it can be a supertrait of the sealed
/// [`PeerSimulatorBackend`](super::PeerSimulatorBackend), and can't be named outside the crate.
pub trait Backend: Send + Sized + 'static {
    /// Whether this backend has a mempool, and so sends CHIP-26 mempool updates.
    const HAS_MEMPOOL: bool;

    /// Creates the simulator that a new or reset [`PeerSimulator`](crate::PeerSimulator) serves.
    fn new_for_peers(config: &PeerSimulatorConfig) -> Self;

    fn peak_height(&self) -> u32;

    fn peak_header_hash(&self) -> Bytes32;

    fn header_hash_at(&self, height: u32) -> Option<Bytes32>;

    fn coin_states(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState>;

    fn coin_states_by_puzzle_hashes(
        &self,
        puzzle_hashes: &IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState>;

    fn children(&self, coin_id: Bytes32) -> Vec<CoinState>;

    /// The puzzle reveal and solution of a coin that was spent at `height`.
    fn puzzle_and_solution(&self, coin_id: Bytes32, height: u32) -> Option<(Program, Program)>;

    /// Validates a transaction and either includes it in a block or adds it to the mempool.
    ///
    /// Returns the changes since the last call to this or [`Backend::drain_events`], in order,
    /// for the peers to be told about.
    fn submit_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<Vec<PeerEvent>, SimulatorError>;

    /// Returns the changes since the last call to this or [`Backend::submit_transaction`], in
    /// order, for the peers to be told about.
    fn drain_events(&mut self) -> Vec<PeerEvent>;

    fn mempool_items(&self) -> Vec<(Bytes32, MempoolItemIds)>;

    fn mempool_transaction(&self, transaction_id: Bytes32) -> Option<SpendBundle>;
}

/// The instant-inclusion backend, where each transaction is put in its own block right away.
impl Backend for Simulator {
    const HAS_MEMPOOL: bool = false;

    fn new_for_peers(_config: &PeerSimulatorConfig) -> Self {
        Self::default()
    }

    fn peak_height(&self) -> u32 {
        self.height()
    }

    fn peak_header_hash(&self) -> Bytes32 {
        self.header_hash()
    }

    fn header_hash_at(&self, height: u32) -> Option<Bytes32> {
        self.header_hash_of(height)
    }

    fn coin_states(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        self.lookup_coin_ids(coin_ids)
    }

    fn coin_states_by_puzzle_hashes(
        &self,
        puzzle_hashes: &IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        self.lookup_puzzle_hashes(puzzle_hashes.clone(), include_hints)
    }

    fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        Simulator::children(self, coin_id)
    }

    fn puzzle_and_solution(&self, coin_id: Bytes32, height: u32) -> Option<(Program, Program)> {
        let coin_state = self.coin_state(coin_id)?;
        if coin_state.spent_height != Some(height) {
            return None;
        }
        Simulator::puzzle_and_solution(self, coin_id)
    }

    fn submit_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<Vec<PeerEvent>, SimulatorError> {
        let updates = self.new_transaction(spend_bundle)?;
        let coin_states = updates
            .keys()
            .filter_map(|coin_id| Some((self.coin_state(*coin_id)?, self.hints_of(*coin_id))))
            .collect();
        Ok(vec![PeerEvent::Block {
            height: self.height(),
            header_hash: self.header_hash(),
            coin_states,
        }])
    }

    /// The [`Simulator`] doesn't record changes made through it directly, so there is nothing to
    /// drain.
    fn drain_events(&mut self) -> Vec<PeerEvent> {
        Vec::new()
    }

    fn mempool_items(&self) -> Vec<(Bytes32, MempoolItemIds)> {
        Vec::new()
    }

    fn mempool_transaction(&self, _transaction_id: Bytes32) -> Option<SpendBundle> {
        None
    }
}

impl Backend for FullNodeSimulator {
    const HAS_MEMPOOL: bool = true;

    fn new_for_peers(config: &PeerSimulatorConfig) -> Self {
        let mut simulator = Self::new();
        simulator.set_genesis_challenge(config.genesis_challenge);
        simulator.enable_peer_events();
        simulator
    }

    fn peak_height(&self) -> u32 {
        self.height()
    }

    fn peak_header_hash(&self) -> Bytes32 {
        self.header_hash()
    }

    fn header_hash_at(&self, height: u32) -> Option<Bytes32> {
        self.header_hash_of(height)
    }

    fn coin_states(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        self.peer_coin_states(coin_ids)
    }

    fn coin_states_by_puzzle_hashes(
        &self,
        puzzle_hashes: &IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        self.peer_coin_states_by_puzzle_hashes(puzzle_hashes, include_hints)
    }

    fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.peer_children(coin_id)
    }

    fn puzzle_and_solution(&self, coin_id: Bytes32, height: u32) -> Option<(Program, Program)> {
        let coin_state = self.peer_coin_states(&IndexSet::from([coin_id])).pop()?;
        if coin_state.spent_height != Some(height) {
            return None;
        }
        let coin_spend = self.peer_coin_spend(coin_id)?;
        Some((
            coin_spend.puzzle_reveal.clone(),
            coin_spend.solution.clone(),
        ))
    }

    fn submit_transaction(
        &mut self,
        spend_bundle: SpendBundle,
    ) -> Result<Vec<PeerEvent>, SimulatorError> {
        if let Some(error) = self.push_tx_detailed(spend_bundle).error {
            return Err(error);
        }
        Ok(self.drain_peer_events())
    }

    fn drain_events(&mut self) -> Vec<PeerEvent> {
        self.drain_peer_events()
    }

    fn mempool_items(&self) -> Vec<(Bytes32, MempoolItemIds)> {
        self.peer_mempool_items()
    }

    fn mempool_transaction(&self, transaction_id: Bytes32) -> Option<SpendBundle> {
        self.peer_mempool_spend_bundle(transaction_id)
    }
}
