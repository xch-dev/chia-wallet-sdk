use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{self as sdk, PlotNftInfo, PoolState, SingletonInfo};
use chia_sdk_types::puzzles;
use clvm_utils::TreeHash;

use crate::{Clvm, Program, Proof, conditions_to_programs};

/// The parent coin id of a pool reward farmed at the given height.
pub fn pool_reward_parent_id(genesis_challenge: Bytes32, height: u32) -> Result<Bytes32> {
    Ok(puzzles::pool_reward_parent_id(
        puzzles::pool_reward_prefix(genesis_challenge),
        height,
    ))
}

/// The value to use for [`PlotNftInfo::pool_reward_prefix`] on a network.
pub fn pool_reward_prefix(genesis_challenge: Bytes32) -> Result<Bytes32> {
    Ok(puzzles::pool_reward_prefix(genesis_challenge))
}

/// The version of the pool protocol that new pool states are created with.
pub fn pool_protocol_version() -> Result<u8> {
    Ok(sdk::POOL_PROTOCOL_VERSION)
}

/// The delay used by the reference wallet before farmed coins can be swept.
pub fn default_p2_singleton_delay() -> Result<u64> {
    Ok(sdk::DEFAULT_P2_SINGLETON_DELAY)
}

pub trait PoolStateExt: Sized {
    fn self_pooling(target_puzzle_hash: Bytes32, owner_public_key: PublicKey) -> Result<Self>;
    fn farming_to_pool(
        target_puzzle_hash: Bytes32,
        owner_public_key: PublicKey,
        pool_url: String,
        relative_lock_height: u32,
    ) -> Result<Self>;
    fn is_waiting_room(&self) -> Result<bool>;
}

impl PoolStateExt for PoolState {
    fn self_pooling(target_puzzle_hash: Bytes32, owner_public_key: PublicKey) -> Result<Self> {
        Ok(Self::self_pooling(target_puzzle_hash, owner_public_key))
    }

    fn farming_to_pool(
        target_puzzle_hash: Bytes32,
        owner_public_key: PublicKey,
        pool_url: String,
        relative_lock_height: u32,
    ) -> Result<Self> {
        Ok(Self::farming_to_pool(
            target_puzzle_hash,
            owner_public_key,
            pool_url,
            relative_lock_height,
        ))
    }

    fn is_waiting_room(&self) -> Result<bool> {
        Ok(PoolState::is_waiting_room(self))
    }
}

pub trait PlotNftInfoExt {
    fn p2_puzzle_hash(&self) -> Result<TreeHash>;
    fn inner_puzzle_hash(&self) -> Result<TreeHash>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
    fn claim_delayed(&self, clvm: Clvm, coin: Coin) -> Result<Coin>;
}

impl PlotNftInfoExt for PlotNftInfo {
    fn p2_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(PlotNftInfo::p2_puzzle_hash(self))
    }

    fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::inner_puzzle_hash(self))
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::puzzle_hash(self))
    }

    fn claim_delayed(&self, clvm: Clvm, coin: Coin) -> Result<Coin> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(PlotNftInfo::claim_delayed(self, &mut ctx, coin)?)
    }
}

#[derive(Clone)]
pub struct PlotNft {
    pub coin: Coin,
    pub proof: Proof,
    pub info: PlotNftInfo,
}

impl PlotNft {
    /// Parses a plot NFT from the solution of its launcher coin.
    pub fn parse_launcher(
        launcher_coin: Coin,
        solution: Program,
        genesis_challenge: Bytes32,
    ) -> Result<Option<Self>> {
        let ctx = solution.0.lock().unwrap();

        Ok(
            sdk::PlotNft::parse_launcher(&ctx, launcher_coin, solution.1, genesis_challenge)?
                .map(Into::into),
        )
    }

    /// Parses the child of this plot NFT from the solution it was spent with.
    pub fn parse_child(&self, solution: Program) -> Result<Option<Self>> {
        let ctx = solution.0.lock().unwrap();

        Ok(sdk::PlotNft::from(self.clone())
            .parse_child(&ctx, solution.1)?
            .map(Into::into))
    }

    pub fn leave_pool(&self, clvm: Clvm) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(sdk::PlotNft::from(self.clone())
            .leave_pool(&mut ctx)?
            .into())
    }

    pub fn join_pool(
        &self,
        clvm: Clvm,
        target_puzzle_hash: Bytes32,
        pool_url: String,
        relative_lock_height: u32,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(sdk::PlotNft::from(self.clone())
            .join_pool(&mut ctx, target_puzzle_hash, pool_url, relative_lock_height)?
            .into())
    }

    pub fn self_pool(&self, clvm: Clvm, target_puzzle_hash: Bytes32) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(sdk::PlotNft::from(self.clone())
            .self_pool(&mut ctx, target_puzzle_hash)?
            .into())
    }

    pub fn claim_rewards(&self, clvm: Clvm, rewards: Vec<Coin>) -> Result<PlotNftClaim> {
        let mut ctx = clvm.0.lock().unwrap();

        let (conditions, plot_nft) =
            sdk::PlotNft::from(self.clone()).claim_rewards(&mut ctx, &rewards)?;

        Ok(PlotNftClaim {
            plot_nft: plot_nft.into(),
            conditions: conditions_to_programs(&mut ctx, &clvm.0, conditions)?,
        })
    }
}

impl From<sdk::PlotNft> for PlotNft {
    fn from(value: sdk::PlotNft) -> Self {
        Self {
            coin: value.coin,
            proof: value.proof.into(),
            info: value.info,
        }
    }
}

impl From<PlotNft> for sdk::PlotNft {
    fn from(value: PlotNft) -> Self {
        Self::new(value.coin, value.proof.into(), value.info)
    }
}

#[derive(Clone)]
pub struct PlotNftMint {
    pub plot_nft: PlotNft,
    pub parent_conditions: Vec<Program>,
}

#[derive(Clone)]
pub struct PlotNftClaim {
    pub plot_nft: PlotNft,
    pub conditions: Vec<Program>,
}
