use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::SingletonV1Args;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{
    P2SingletonOrDelayedLayer, PoolMemberLayer, PoolState, PoolWaitingRoomLayer, SingletonInfo,
};

/// Information needed to construct the outer puzzle of a plot NFT.
///
/// The seconds delay and delayed puzzle hash are only revealed by the launcher spend. The inner
/// puzzles commit to them through the p2 puzzle hash, so they must be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotNftInfo {
    /// The coin id of the launcher coin that created this plot NFT's singleton.
    pub launcher_id: Bytes32,
    /// The current state, which determines the inner puzzle.
    pub pool_state: PoolState,
    /// The number of seconds that coins locked to the p2 puzzle hash must exist before they can be
    /// sent to the delayed puzzle hash.
    pub seconds_delay: u64,
    /// The puzzle hash that coins locked to the p2 puzzle hash can be sent to after the delay.
    pub delayed_puzzle_hash: Bytes32,
    /// The first half of the genesis challenge followed by zeros, for recognizing pool rewards.
    pub pool_reward_prefix: Bytes32,
}

impl PlotNftInfo {
    pub fn new(
        launcher_id: Bytes32,
        pool_state: PoolState,
        seconds_delay: u64,
        delayed_puzzle_hash: Bytes32,
        pool_reward_prefix: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            pool_state,
            seconds_delay,
            delayed_puzzle_hash,
            pool_reward_prefix,
        }
    }

    /// The same plot NFT, in a new state.
    #[must_use]
    pub fn with_pool_state(&self, pool_state: PoolState) -> Self {
        Self {
            pool_state,
            ..self.clone()
        }
    }

    /// The layer for coins that are farmed to this plot NFT.
    pub fn p2_layer(&self) -> P2SingletonOrDelayedLayer {
        P2SingletonOrDelayedLayer::new(
            self.launcher_id,
            self.seconds_delay,
            self.delayed_puzzle_hash,
        )
    }

    /// The address that plots farm to, which doesn't change when the state does. This is not the
    /// target puzzle hash, which is where rewards are paid to when they're claimed.
    pub fn p2_puzzle_hash(&self) -> TreeHash {
        self.p2_layer().tree_hash()
    }

    /// The waiting room puzzle for the current state. This is the inner puzzle unless it's
    /// [`PoolSingletonState::FarmingToPool`](crate::PoolSingletonState::FarmingToPool), in which
    /// case it's the puzzle that the plot NFT can leave the pool with.
    pub fn waiting_room_layer(&self) -> PoolWaitingRoomLayer {
        PoolWaitingRoomLayer {
            pool_puzzle_hash: self.pool_state.target_puzzle_hash,
            p2_singleton_puzzle_hash: self.p2_puzzle_hash().into(),
            owner_public_key: self.pool_state.owner_public_key,
            pool_reward_prefix: self.pool_reward_prefix,
            relative_lock_height: self.pool_state.relative_lock_height,
        }
    }

    /// The pool member puzzle for the current state.
    pub fn member_layer(&self) -> PoolMemberLayer {
        PoolMemberLayer {
            pool_puzzle_hash: self.pool_state.target_puzzle_hash,
            p2_singleton_puzzle_hash: self.p2_puzzle_hash().into(),
            owner_public_key: self.pool_state.owner_public_key,
            pool_reward_prefix: self.pool_reward_prefix,
            waiting_room_puzzle_hash: self.waiting_room_layer().tree_hash().into(),
        }
    }
}

impl SingletonInfo for PlotNftInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        if self.pool_state.is_waiting_room() {
            self.waiting_room_layer().tree_hash()
        } else {
            self.member_layer().tree_hash()
        }
    }

    fn puzzle_hash(&self) -> TreeHash {
        SingletonV1Args::puzzle_hash(self.launcher_id, self.inner_puzzle_hash())
    }
}
