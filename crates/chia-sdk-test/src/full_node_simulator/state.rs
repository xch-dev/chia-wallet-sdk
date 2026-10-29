use chia_protocol::{Bytes32, CoinSpend};
use indexmap::{IndexMap, IndexSet};

use crate::{ChainStateError, SimulatorError};

use super::{SimBlock, SimCoinRecord};

#[derive(Debug, Clone)]
pub(super) struct CoinChange {
    pub(super) coin_id: Bytes32,
    pub(super) before: Option<SimCoinRecord>,
    pub(super) after: Option<SimCoinRecord>,
}

#[derive(Debug, Clone)]
pub(super) struct SpendChange {
    pub(super) coin_id: Bytes32,
    pub(super) before: Option<CoinSpend>,
    pub(super) after: Option<CoinSpend>,
}

#[derive(Debug, Clone)]
pub(super) struct HintChange {
    pub(super) coin_id: Bytes32,
    pub(super) before: Option<Bytes32>,
    pub(super) after: Option<Bytes32>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockDelta {
    pub(super) coins: Vec<CoinChange>,
    pub(super) spends: Vec<SpendChange>,
    pub(super) hints: Vec<HintChange>,
}

#[derive(Debug, Clone)]
pub(super) struct ChainState {
    pub(super) height: u32,
    pub(super) next_timestamp: u64,
    pub(super) header_hashes: Vec<Bytes32>,
    pub(super) blocks: IndexMap<Bytes32, SimBlock>,
    pub(super) coins: IndexMap<Bytes32, SimCoinRecord>,
    pub(super) coin_spends: IndexMap<Bytes32, CoinSpend>,
    pub(super) coin_hints: IndexMap<Bytes32, Bytes32>,
}

impl ChainState {
    pub(super) fn new(
        height: u32,
        next_timestamp: u64,
        header_hashes: Vec<Bytes32>,
        blocks: IndexMap<Bytes32, SimBlock>,
        coins: IndexMap<Bytes32, SimCoinRecord>,
        coin_spends: IndexMap<Bytes32, CoinSpend>,
        coin_hints: IndexMap<Bytes32, Bytes32>,
    ) -> Self {
        Self {
            height,
            next_timestamp,
            header_hashes,
            blocks,
            coins,
            coin_spends,
            coin_hints,
        }
    }

    pub(super) fn apply_block(&mut self, block: SimBlock) -> Result<(), SimulatorError> {
        self.validate_apply(&block)?;

        for change in &block.delta.coins {
            set_entry(&mut self.coins, change.coin_id, change.after);
        }
        for change in &block.delta.spends {
            set_entry(&mut self.coin_spends, change.coin_id, change.after.clone());
        }
        for change in &block.delta.hints {
            set_entry(&mut self.coin_hints, change.coin_id, change.after);
        }

        let header_hash = block.record.header_hash;
        self.height = block.record.height;
        self.next_timestamp = block
            .record
            .timestamp
            .unwrap_or(self.next_timestamp)
            .saturating_add(1);
        self.header_hashes.push(header_hash);
        self.blocks.insert(header_hash, block);
        Ok(())
    }

    pub(super) fn revert_tip(&mut self) -> Result<Option<SimBlock>, SimulatorError> {
        if self.height == 0 {
            return Ok(None);
        }
        let Some(header_hash) = self.header_hashes.last().copied() else {
            return Err(ChainStateError::MissingTipHeader.into());
        };
        let Some(block) = self.blocks.get(&header_hash) else {
            return Err(ChainStateError::MissingTipBlock.into());
        };
        self.validate_revert(block)?;

        let block = self
            .blocks
            .swap_remove(&header_hash)
            .expect("tip existence was validated");
        self.header_hashes.pop();
        for change in block.delta.hints.iter().rev() {
            set_entry(&mut self.coin_hints, change.coin_id, change.before);
        }
        for change in block.delta.spends.iter().rev() {
            set_entry(&mut self.coin_spends, change.coin_id, change.before.clone());
        }
        for change in block.delta.coins.iter().rev() {
            set_entry(&mut self.coins, change.coin_id, change.before);
        }
        self.height = self.height.saturating_sub(1);
        self.next_timestamp = block.record.timestamp.unwrap_or(self.next_timestamp);
        Ok(Some(block))
    }

    pub(super) fn insert_manual_coin(&mut self, coin_id: Bytes32, record: SimCoinRecord) {
        self.coins.insert(coin_id, record);
    }

    fn validate_apply(&self, block: &SimBlock) -> Result<(), SimulatorError> {
        if block.record.height != self.height.saturating_add(1) {
            return Err(ChainStateError::InvalidBlockHeight.into());
        }
        if block.record.prev_hash != self.header_hash() {
            return Err(ChainStateError::InvalidPreviousHash.into());
        }
        if block.record.timestamp != Some(self.next_timestamp) {
            return Err(ChainStateError::InvalidBlockTimestamp.into());
        }
        if self.blocks.contains_key(&block.record.header_hash) {
            return Err(ChainStateError::DuplicateBlockHeader.into());
        }
        validate_unique_changes(&block.delta)?;
        validate_current_values(self, &block.delta, false)
    }

    fn validate_revert(&self, block: &SimBlock) -> Result<(), SimulatorError> {
        if block.record.height != self.height || block.record.header_hash != self.header_hash() {
            return Err(ChainStateError::BlockIsNotTip.into());
        }
        validate_unique_changes(&block.delta)?;
        validate_current_values(self, &block.delta, true)
    }

    fn header_hash(&self) -> Bytes32 {
        self.header_hashes.last().copied().unwrap_or_default()
    }
}

fn validate_unique_changes(delta: &BlockDelta) -> Result<(), SimulatorError> {
    let mut coin_ids = IndexSet::new();
    if delta
        .coins
        .iter()
        .any(|change| !coin_ids.insert(change.coin_id))
    {
        return Err(ChainStateError::DuplicateCoinChange.into());
    }
    let mut spend_ids = IndexSet::new();
    if delta
        .spends
        .iter()
        .any(|change| !spend_ids.insert(change.coin_id))
    {
        return Err(ChainStateError::DuplicateSpendChange.into());
    }
    let mut hint_ids = IndexSet::new();
    if delta
        .hints
        .iter()
        .any(|change| !hint_ids.insert(change.coin_id))
    {
        return Err(ChainStateError::DuplicateHintChange.into());
    }
    Ok(())
}

fn validate_current_values(
    state: &ChainState,
    delta: &BlockDelta,
    use_after: bool,
) -> Result<(), SimulatorError> {
    for change in &delta.coins {
        let expected = if use_after {
            change.after.as_ref()
        } else {
            change.before.as_ref()
        };
        if state.coins.get(&change.coin_id) != expected {
            return Err(ChainStateError::CoinStateMismatch.into());
        }
    }
    for change in &delta.spends {
        let expected = if use_after {
            change.after.as_ref()
        } else {
            change.before.as_ref()
        };
        if state.coin_spends.get(&change.coin_id) != expected {
            return Err(ChainStateError::CoinSpendStateMismatch.into());
        }
    }
    for change in &delta.hints {
        let expected = if use_after {
            change.after.as_ref()
        } else {
            change.before.as_ref()
        };
        if state.coin_hints.get(&change.coin_id) != expected {
            return Err(ChainStateError::CoinHintStateMismatch.into());
        }
    }
    Ok(())
}

fn set_entry<T>(map: &mut IndexMap<Bytes32, T>, key: Bytes32, value: Option<T>) {
    if let Some(value) = value {
        map.insert(key, value);
    } else {
        map.swap_remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use crate::{FullNodeSimulator, to_puzzle};

    #[test]
    fn apply_then_revert_restores_exact_chain_state() {
        let mut sim = FullNodeSimulator::new();
        let (puzzle_hash, _) = to_puzzle(1).unwrap();
        let manual_coin = sim.new_coin(puzzle_hash, 100);
        let before = sim.state.clone();

        sim.farm_block(1);
        let reverted = sim.state.revert_tip().unwrap().unwrap();

        assert_eq!(reverted.record.height, before.height + 1);
        assert_eq!(sim.state.height, before.height);
        assert_eq!(sim.state.next_timestamp, before.next_timestamp);
        assert_eq!(sim.state.header_hashes, before.header_hashes);
        assert_eq!(
            sim.state.blocks.keys().collect::<Vec<_>>(),
            before.blocks.keys().collect::<Vec<_>>()
        );
        assert_eq!(sim.state.coins, before.coins);
        assert_eq!(sim.state.coin_spends, before.coin_spends);
        assert_eq!(sim.state.coin_hints, before.coin_hints);
        assert_eq!(
            sim.state.coins.get(&manual_coin.coin_id()),
            before.coins.get(&manual_coin.coin_id())
        );
    }

    #[test]
    fn rejected_block_delta_does_not_mutate_chain_state() {
        let mut sim = FullNodeSimulator::new();
        sim.farm_block(1);
        let mut block = sim.state.revert_tip().unwrap().unwrap();
        block.record.prev_hash = block.record.header_hash;
        let before = sim.state.clone();

        assert!(sim.state.apply_block(block).is_err());
        assert_eq!(sim.state.height, before.height);
        assert_eq!(sim.state.next_timestamp, before.next_timestamp);
        assert_eq!(sim.state.header_hashes, before.header_hashes);
        assert_eq!(
            sim.state.blocks.keys().collect::<Vec<_>>(),
            before.blocks.keys().collect::<Vec<_>>()
        );
        assert_eq!(sim.state.coins, before.coins);
        assert_eq!(sim.state.coin_spends, before.coin_spends);
        assert_eq!(sim.state.coin_hints, before.coin_hints);
    }
}
