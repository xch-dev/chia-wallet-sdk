use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_streamable_macro::Streamable;
use chia_traits::Streamable;
use clvmr::{Allocator, NodePtr, SExp};

use crate::DriverError;

/// The version of the pool protocol that new [`PoolState`] values are created with.
pub const POOL_PROTOCOL_VERSION: u8 = 1;

/// The lowest relative lock height that a pool can use, as enforced by the reference wallet.
pub const MINIMUM_RELATIVE_LOCK_HEIGHT: u32 = 5;

/// The highest relative lock height that a pool can use, as enforced by the reference wallet.
pub const MAXIMUM_RELATIVE_LOCK_HEIGHT: u32 = 1000;

/// The key of the serialized [`PoolState`] in launcher and travel spend key value lists.
pub(crate) const POOL_STATE_KEY: &[u8] = b"p";

/// What a plot NFT is currently doing, which is stored on-chain as a [`u8`].
#[repr(u8)]
#[derive(Streamable, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolSingletonState {
    /// Rewards are paid to the owner, and the plot NFT can move at any time.
    SelfPooling = 1,
    /// The plot NFT is waiting for the relative lock height before it can leave a pool.
    LeavingPool = 2,
    /// Rewards are paid to a pool.
    FarmingToPool = 3,
}

/// The state of a plot NFT, serialized on-chain in the same format as `PoolState` in the
/// reference wallet. It's revealed in the launcher spend and in the spends which change state, so
/// spends that only claim rewards don't reveal it.
///
/// The URL and the difference between [`PoolSingletonState::SelfPooling`] and
/// [`PoolSingletonState::LeavingPool`] aren't committed to by the puzzle.
#[derive(Streamable, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolState {
    pub version: u8,
    pub state: PoolSingletonState,
    /// The puzzle hash that rewards are paid to. Not to be confused with the p2 puzzle hash, which
    /// is the address that plots farm to.
    pub target_puzzle_hash: Bytes32,
    /// The key that controls the plot NFT, which can't be changed. This is not a synthetic key.
    pub owner_public_key: PublicKey,
    /// The URL of the pool. The reference wallet uses both [`None`] and an empty string when self
    /// pooling, so which one was used needs to be preserved.
    pub pool_url: Option<String>,
    /// The number of blocks that the plot NFT must wait in the waiting room before leaving.
    pub relative_lock_height: u32,
}

impl PoolState {
    pub fn new(
        version: u8,
        state: PoolSingletonState,
        target_puzzle_hash: Bytes32,
        owner_public_key: PublicKey,
        pool_url: Option<String>,
        relative_lock_height: u32,
    ) -> Self {
        Self {
            version,
            state,
            target_puzzle_hash,
            owner_public_key,
            pool_url,
            relative_lock_height,
        }
    }

    /// Creates a state for a plot NFT that pays rewards to the target puzzle hash.
    pub fn self_pooling(target_puzzle_hash: Bytes32, owner_public_key: PublicKey) -> Self {
        Self {
            version: POOL_PROTOCOL_VERSION,
            state: PoolSingletonState::SelfPooling,
            target_puzzle_hash,
            owner_public_key,
            pool_url: None,
            relative_lock_height: 0,
        }
    }

    /// Creates a state for a plot NFT that farms to a pool. The target puzzle hash and relative
    /// lock height are provided by the pool.
    pub fn farming_to_pool(
        target_puzzle_hash: Bytes32,
        owner_public_key: PublicKey,
        pool_url: String,
        relative_lock_height: u32,
    ) -> Self {
        Self {
            version: POOL_PROTOCOL_VERSION,
            state: PoolSingletonState::FarmingToPool,
            target_puzzle_hash,
            owner_public_key,
            pool_url: Some(pool_url),
            relative_lock_height,
        }
    }

    /// The state that this one moves to when leaving a pool, which only changes what it's doing.
    #[must_use]
    pub fn leaving(&self) -> Self {
        Self {
            version: POOL_PROTOCOL_VERSION,
            state: PoolSingletonState::LeavingPool,
            ..self.clone()
        }
    }

    /// Whether the plot NFT uses the waiting room puzzle in this state, rather than the pool member
    /// puzzle.
    pub fn is_waiting_room(&self) -> bool {
        self.state != PoolSingletonState::FarmingToPool
    }

    /// Applies the same checks as the reference wallet does to a state it's about to move to.
    /// This must not be used on states from the chain, since the puzzle doesn't enforce any of it.
    pub fn validate(&self) -> Result<(), DriverError> {
        if self.version > POOL_PROTOCOL_VERSION {
            return Err(DriverError::InvalidPoolState(
                "the pool protocol version is not supported",
            ));
        }

        // The owner has to sign with this key, and consensus rejects signatures from it
        if self.owner_public_key.is_inf() {
            return Err(DriverError::InvalidPoolState(
                "the owner public key can't be the infinity point",
            ));
        }

        let pool_url = self.pool_url.as_deref().unwrap_or_default();

        if self.state == PoolSingletonState::SelfPooling {
            if !pool_url.is_empty() {
                return Err(DriverError::InvalidPoolState(
                    "self pooling can't have a pool URL",
                ));
            }
            if self.relative_lock_height != 0 {
                return Err(DriverError::InvalidPoolState(
                    "self pooling must have a relative lock height of 0",
                ));
            }
            return Ok(());
        }

        if pool_url.is_empty() {
            return Err(DriverError::InvalidPoolState("the pool URL is empty"));
        }

        if !(MINIMUM_RELATIVE_LOCK_HEIGHT..=MAXIMUM_RELATIVE_LOCK_HEIGHT)
            .contains(&self.relative_lock_height)
        {
            return Err(DriverError::InvalidPoolState(
                "the relative lock height must be between 5 and 1000",
            ));
        }

        Ok(())
    }

    /// Reads the state out of a key value list, or returns [`None`] if there isn't a valid one.
    pub fn from_key_value_list(allocator: &Allocator, list: NodePtr) -> Option<Self> {
        let value = key_value_pairs(allocator, list)
            .into_iter()
            .find_map(|(key, value)| {
                (allocator.atom(key).as_ref() == POOL_STATE_KEY).then_some(value)
            })?;

        let SExp::Atom = allocator.sexp(value) else {
            return None;
        };

        Self::from_bytes(allocator.atom(value).as_ref()).ok()
    }

    /// The key value list entry containing this state.
    pub(crate) fn key_value(&self) -> Result<(Bytes, Bytes), DriverError> {
        Ok((POOL_STATE_KEY.to_vec().into(), self.to_bytes()?.into()))
    }
}

/// The entries of a key value list. Anything that isn't a pair with an atom as the key is skipped.
pub(crate) fn key_value_pairs(allocator: &Allocator, mut list: NodePtr) -> Vec<(NodePtr, NodePtr)> {
    let mut pairs = Vec::new();

    while let SExp::Pair(first, rest) = allocator.sexp(list) {
        list = rest;

        if let SExp::Pair(key, value) = allocator.sexp(first)
            && let SExp::Atom = allocator.sexp(key)
        {
            pairs.push((key, value));
        }
    }

    pairs
}
