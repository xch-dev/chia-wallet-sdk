use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{
    P2_SINGLETON_OR_DELAYED_PUZHASH, P2_SINGLETON_OR_DELAYED_PUZHASH_HASH, POOL_MEMBER_INNERPUZ,
    POOL_MEMBER_INNERPUZ_HASH, POOL_WAITINGROOM_INNERPUZ, POOL_WAITINGROOM_INNERPUZ_HASH,
    SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER, SINGLETON_TOP_LAYER_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// The original singleton top layer, which plot NFTs are locked to forever. Everything else in the
/// SDK uses v1.1 through [`SingletonArgs`](chia_puzzle_types::singleton::SingletonArgs), which has
/// a different mod hash and therefore can't be used to construct or parse plot NFTs.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct SingletonV1Args<I> {
    pub singleton_struct: SingletonStruct,
    pub inner_puzzle: I,
}

impl<I> SingletonV1Args<I> {
    pub fn new(launcher_id: Bytes32, inner_puzzle: I) -> Self {
        Self {
            singleton_struct: singleton_v1_struct(launcher_id),
            inner_puzzle,
        }
    }
}

impl SingletonV1Args<TreeHash> {
    pub fn puzzle_hash(launcher_id: Bytes32, inner_puzzle_hash: TreeHash) -> TreeHash {
        Self::new(launcher_id, inner_puzzle_hash).curry_tree_hash()
    }
}

impl<I> Mod for SingletonV1Args<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SINGLETON_TOP_LAYER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(SINGLETON_TOP_LAYER_HASH)
    }
}

/// The [`SingletonStruct`] for a singleton using the v1.0 top layer.
pub fn singleton_v1_struct(launcher_id: Bytes32) -> SingletonStruct {
    SingletonStruct {
        mod_hash: SINGLETON_TOP_LAYER_HASH.into(),
        launcher_id,
        launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
    }
}

/// The inner puzzle of a plot NFT that is farming to a pool. Pool rewards can be claimed by anyone,
/// and the owner can start leaving by moving to the waiting room puzzle.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PoolMemberArgs {
    pub pool_puzzle_hash: Bytes32,
    pub p2_singleton_puzzle_hash: Bytes32,
    pub owner_public_key: PublicKey,
    pub pool_reward_prefix: Bytes32,
    pub waiting_room_puzzle_hash: Bytes32,
}

impl Mod for PoolMemberArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&POOL_MEMBER_INNERPUZ)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(POOL_MEMBER_INNERPUZ_HASH)
    }
}

/// Claims a pool reward. The puzzle treats a nil height as [`PoolMemberEscapeSolution`], and a height
/// of 0 is encoded as nil here, so this type can't be used to claim a reward from height 0.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PoolMemberAbsorbSolution {
    pub pool_reward_amount: u64,
    pub pool_reward_height: u32,
}

/// Moves to the waiting room. The owner signs the tree hash of the extra data.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PoolMemberEscapeSolution<T> {
    pub extra_data: T,
    pub pool_reward_height: (),
}

/// The inner puzzle of a plot NFT that is self pooling or leaving a pool. Pool rewards can be
/// claimed by anyone, and the owner can move to any puzzle after the relative lock height.
///
/// Claiming a reward creates a new coin, so it restarts the relative lock height.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PoolWaitingRoomArgs {
    pub pool_puzzle_hash: Bytes32,
    pub p2_singleton_puzzle_hash: Bytes32,
    pub owner_public_key: PublicKey,
    pub pool_reward_prefix: Bytes32,
    pub relative_lock_height: u32,
}

impl Mod for PoolWaitingRoomArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&POOL_WAITINGROOM_INNERPUZ)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(POOL_WAITINGROOM_INNERPUZ_HASH)
    }
}

/// Claims a pool reward.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PoolWaitingRoomAbsorbSolution {
    pub spend_type: (),
    pub pool_reward_amount: u64,
    pub pool_reward_height: u32,
}

/// Moves to the destination puzzle. The owner signs the tree hash of `(destination extra_data)`.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PoolWaitingRoomTravelSolution<T> {
    pub spend_type: u8,
    pub extra_data: T,
    pub destination_puzzle_hash: Bytes32,
}

/// The puzzle that block rewards are farmed to, which is what plots commit to. Pool rewards can be
/// claimed by the singleton, and any coin can be swept to the delayed puzzle hash after the delay.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2SingletonOrDelayedArgs {
    pub singleton_mod_hash: Bytes32,
    pub launcher_id: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
    pub seconds_delay: u64,
    pub delayed_puzzle_hash: Bytes32,
}

impl P2SingletonOrDelayedArgs {
    pub fn new(launcher_id: Bytes32, seconds_delay: u64, delayed_puzzle_hash: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_HASH.into(),
            launcher_id,
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
            seconds_delay,
            delayed_puzzle_hash,
        }
    }
}

impl Mod for P2SingletonOrDelayedArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_SINGLETON_OR_DELAYED_PUZHASH)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(P2_SINGLETON_OR_DELAYED_PUZHASH_HASH)
    }
}

/// Claims the coin with the singleton.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2SingletonClaimSolution {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub my_id: Bytes32,
}

/// Sweeps the coin to the delayed puzzle hash.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2SingletonDelayedSolution {
    pub amount: u64,
    pub my_id: (),
}

/// The value curried into the pool puzzles to recognize pool rewards on a network.
pub fn pool_reward_prefix(genesis_challenge: Bytes32) -> Bytes32 {
    let mut prefix = [0; 32];
    prefix[..16].copy_from_slice(&genesis_challenge[..16]);
    prefix.into()
}

/// The parent coin id of a pool reward farmed at the given height.
pub fn pool_reward_parent_id(pool_reward_prefix: Bytes32, height: u32) -> Bytes32 {
    let mut parent = pool_reward_prefix.to_bytes();
    parent[28..].copy_from_slice(&height.to_be_bytes());
    parent.into()
}

/// The height a coin was farmed at, if its parent coin id is that of a pool reward.
pub fn pool_reward_height(pool_reward_prefix: Bytes32, parent_coin_id: Bytes32) -> Option<u32> {
    let height = u32::from_be_bytes(parent_coin_id[28..].try_into().unwrap());
    (pool_reward_parent_id(pool_reward_prefix, height) == parent_coin_id).then_some(height)
}

#[cfg(test)]
mod tests {
    use hex_literal::hex;

    use super::*;

    #[test]
    fn test_pool_reward_parent_id() {
        // The parent of the mainnet pool reward from height 687644
        let genesis = hex!("ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb");
        let parent = hex!("ccd5bb71183532bff220ba46c268991a000000000000000000000000000a7e1c");

        let prefix = pool_reward_prefix(genesis.into());

        assert_eq!(pool_reward_parent_id(prefix, 687_644), parent.into());
        assert_eq!(pool_reward_height(prefix, parent.into()), Some(687_644));

        // Farmer rewards use the other half of the genesis challenge
        let farmer = hex!("3ff07eb358e8255a65c30a2dce0e5fbb000000000000000000000000000a7e1c");
        assert_eq!(pool_reward_height(prefix, farmer.into()), None);
        assert_eq!(pool_reward_height(prefix, Bytes32::default()), None);
    }
}
