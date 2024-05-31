use std::{collections::HashSet, net::SocketAddr};

use chia_bls::{aggregate_verify, PublicKey};
use chia_consensus::gen::{
    conditions::EmptyVisitor,
    flags::MEMPOOL_MODE,
    owned_conditions::OwnedSpendBundleConditions,
    run_block_generator::run_block_generator,
    solution_generator::solution_generator,
    validation_error::{ErrorCode, ValidationErr},
};
use chia_protocol::{Bytes32, Coin, CoinState, PuzzleSolutionResponse, SpendBundle};
use chia_sdk_signer::RequiredSignature;
use clvmr::{
    sha2::{Digest, Sha256},
    Allocator, NodePtr,
};
use indexmap::{IndexMap, IndexSet};
use tokio::sync::MutexGuard;

use super::{simulator_config::SimulatorConfig, simulator_error::SimulatorError};

#[derive(Debug, Default, Clone)]
pub(crate) struct SimulatorData {
    height: u32,
    coin_states: IndexMap<Bytes32, CoinState>,
    hinted_coins: IndexMap<Bytes32, IndexSet<Bytes32>>,
    puzzle_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
    coin_subscriptions: IndexMap<SocketAddr, IndexSet<Bytes32>>,
    puzzle_and_solutions: IndexMap<Bytes32, PuzzleSolutionResponse>,
}

impl SimulatorData {
    pub(crate) fn create_coin(&mut self, coin: Coin) {
        self.coin_states.insert(
            coin.coin_id(),
            CoinState::new(coin, None, Some(self.height)),
        );
    }

    pub(crate) fn add_hint(&mut self, coin_id: Bytes32, hint: Bytes32) {
        self.hinted_coins.entry(hint).or_default().insert(coin_id);
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn header_hash(&self, height: u32) -> Bytes32 {
        let mut hasher = Sha256::new();
        hasher.update(height.to_be_bytes());
        Bytes32::new(hasher.finalize().into())
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn lookup_coin_ids(&self, coin_ids: &IndexSet<Bytes32>) -> Vec<CoinState> {
        coin_ids
            .iter()
            .filter_map(|coin_id| self.coin_states.get(coin_id).copied())
            .collect()
    }

    pub(crate) fn lookup_puzzle_hashes(
        &self,
        puzzle_hashes: IndexSet<Bytes32>,
        include_hints: bool,
    ) -> Vec<CoinState> {
        let mut coin_states = IndexMap::new();

        for (coin_id, coin_state) in &self.coin_states {
            if puzzle_hashes.contains(&coin_state.coin.puzzle_hash) {
                coin_states.insert(*coin_id, self.coin_states[coin_id]);
            }
        }

        if include_hints {
            for puzzle_hash in puzzle_hashes {
                if let Some(hinted_coins) = self.hinted_coins.get(&puzzle_hash) {
                    for coin_id in hinted_coins {
                        coin_states.insert(*coin_id, self.coin_states[coin_id]);
                    }
                }
            }
        }

        coin_states.into_values().collect()
    }

    pub(crate) fn add_coin_subscriptions(&mut self, peer: SocketAddr, coin_ids: IndexSet<Bytes32>) {
        self.coin_subscriptions
            .entry(peer)
            .or_default()
            .extend(coin_ids);
    }

    pub(crate) fn add_puzzle_subscriptions(
        &mut self,
        peer: SocketAddr,
        puzzle_hashes: IndexSet<Bytes32>,
    ) {
        self.puzzle_subscriptions
            .entry(peer)
            .or_default()
            .extend(puzzle_hashes);
    }

    pub(crate) fn subscription_count(&self, peer: SocketAddr) -> usize {
        self.coin_subscriptions.get(&peer).map_or(0, IndexSet::len)
            + self
                .puzzle_subscriptions
                .get(&peer)
                .map_or(0, IndexSet::len)
    }

    pub(crate) fn puzzle_and_solution(&self, coin_id: Bytes32) -> Option<PuzzleSolutionResponse> {
        self.puzzle_and_solutions.get(&coin_id).cloned()
    }

    pub(crate) fn children(&self, coin_id: Bytes32) -> Vec<CoinState> {
        self.coin_states
            .values()
            .filter(|cs| cs.coin.parent_coin_info == coin_id)
            .copied()
            .collect()
    }

    pub(crate) fn coin_state(&self, coin_id: Bytes32) -> Option<CoinState> {
        self.coin_states.get(&coin_id).copied()
    }
}

pub(crate) fn new_transaction(
    config: &SimulatorConfig,
    data: &mut MutexGuard<'_, SimulatorData>,
    spend_bundle: SpendBundle,
    max_cost: u64,
) -> Result<IndexMap<SocketAddr, IndexSet<CoinState>>, SimulatorError> {
    if spend_bundle.coin_spends.is_empty() {
        return Err(SimulatorError::Validation(ValidationErr(
            NodePtr::NIL,
            ErrorCode::InvalidSpendBundle,
        )));
    }

    let mut allocator = Allocator::new();

    let generator = solution_generator(
        spend_bundle
            .coin_spends
            .iter()
            .cloned()
            .map(|spend| (spend.coin, spend.puzzle_reveal, spend.solution)),
    )?;

    let conds = run_block_generator::<&[u8], EmptyVisitor>(
        &mut allocator,
        &generator,
        &[],
        max_cost,
        MEMPOOL_MODE,
    )?;

    let conds = OwnedSpendBundleConditions::from(&allocator, conds)?;

    let puzzle_hashes: HashSet<Bytes32> =
        conds.spends.iter().map(|spend| spend.puzzle_hash).collect();

    let bundle_puzzle_hashes: HashSet<Bytes32> = spend_bundle
        .coin_spends
        .iter()
        .map(|cs| cs.coin.puzzle_hash)
        .collect();

    if puzzle_hashes != bundle_puzzle_hashes {
        return Err(SimulatorError::Validation(ValidationErr(
            NodePtr::NIL,
            ErrorCode::InvalidSpendBundle,
        )));
    }

    let required_signatures = RequiredSignature::from_coin_spends(
        &mut allocator,
        &spend_bundle.coin_spends,
        config.genesis_challenge,
    )?;

    if !aggregate_verify(
        &spend_bundle.aggregated_signature,
        required_signatures
            .into_iter()
            .map(|required| (required.public_key(), required.final_message()))
            .collect::<Vec<(PublicKey, Vec<u8>)>>(),
    ) {
        return Err(SimulatorError::Validation(ValidationErr(
            NodePtr::NIL,
            ErrorCode::BadAggregateSignature,
        )));
    }

    let mut removed_coins = IndexMap::new();
    let mut added_coins = IndexMap::new();
    let mut added_hints = IndexMap::new();
    let mut puzzle_solutions = IndexMap::new();

    for coin_spend in spend_bundle.coin_spends {
        puzzle_solutions.insert(
            coin_spend.coin.coin_id(),
            PuzzleSolutionResponse {
                coin_name: coin_spend.coin.coin_id(),
                height: data.height,
                puzzle: coin_spend.puzzle_reveal,
                solution: coin_spend.solution,
            },
        );
    }

    // Calculate additions and removals.
    for spend in &conds.spends {
        for new_coin in &spend.create_coin {
            let coin = Coin::new(spend.coin_id, new_coin.0, new_coin.1);

            added_coins.insert(
                coin.coin_id(),
                CoinState::new(coin, None, Some(data.height)),
            );

            let Some(hint) = new_coin.2.clone() else {
                continue;
            };

            if hint.len() != 32 {
                continue;
            }

            added_hints
                .entry(Bytes32::try_from(hint).unwrap())
                .or_insert_with(IndexSet::new)
                .insert(coin.coin_id());
        }

        let coin = Coin::new(spend.parent_id, spend.puzzle_hash, spend.coin_amount);

        let coin_state = data
            .coin_states
            .get(&spend.coin_id)
            .copied()
            .unwrap_or(CoinState::new(coin, None, Some(data.height)));

        removed_coins.insert(spend.coin_id, coin_state);
    }

    // Validate removals.
    for (coin_id, coin_state) in &mut removed_coins {
        let height = data.height;

        if !data.coin_states.contains_key(coin_id) && !added_coins.contains_key(coin_id) {
            return Err(SimulatorError::Validation(ValidationErr(
                NodePtr::NIL,
                ErrorCode::UnknownUnspent,
            )));
        }

        if coin_state.spent_height.is_some() {
            return Err(SimulatorError::Validation(ValidationErr(
                NodePtr::NIL,
                ErrorCode::DoubleSpend,
            )));
        }

        coin_state.spent_height = Some(height);
    }

    // Update the coin data.
    let mut updates = added_coins.clone();
    updates.extend(removed_coins);
    data.height += 1;
    data.coin_states.extend(updates.clone());
    data.hinted_coins.extend(added_hints.clone());
    data.puzzle_and_solutions.extend(puzzle_solutions);

    let peers: Vec<SocketAddr> = data
        .puzzle_subscriptions
        .keys()
        .chain(data.coin_subscriptions.keys())
        .copied()
        .collect();

    let mut peer_updates = IndexMap::new();

    // Send updates to peers.
    for peer in peers {
        let mut coin_states = IndexSet::new();

        let coin_subscriptions = data
            .coin_subscriptions
            .get(&peer)
            .cloned()
            .unwrap_or_default();

        let puzzle_subscriptions = data
            .puzzle_subscriptions
            .get(&peer)
            .cloned()
            .unwrap_or_default();

        for (hint, coins) in &data.hinted_coins {
            let Ok(hint) = hint.to_vec().try_into() else {
                continue;
            };
            let hint = Bytes32::new(hint);

            if puzzle_subscriptions.contains(&hint) {
                coin_states.extend(coins.iter().map(|coin_id| data.coin_states[coin_id]));
            }
        }

        for coin_id in updates.keys() {
            if coin_subscriptions.contains(coin_id)
                || puzzle_subscriptions.contains(&data.coin_states[coin_id].coin.puzzle_hash)
            {
                coin_states.insert(data.coin_states[coin_id]);
            }
        }

        if coin_states.is_empty() {
            continue;
        };

        peer_updates.insert(peer, coin_states);
    }

    Ok(peer_updates)
}
