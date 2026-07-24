use bindy::Result;
use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, singleton::SingletonStruct};
use chia_sdk_driver::{
    RewardDistributorConstants, RewardDistributorType as SdkRewardDistributorType,
};
use chia_sdk_types::{Mod, puzzles::P2NextRewardDistributorEpochArgs};
use clvm_utils::{ToTreeHash, TreeHash};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct P2NextRewardDistributorEpochCoinInfo {
    pub clawback_inner_puzzle_hash: Bytes32,
    pub reward_asset_id: Bytes32,
    pub reward_distributor_launcher_id: Bytes32,
    pub reward_distributor_first_epoch_start: u64,
    pub reward_distributor_epoch_seconds: u64,
}

impl P2NextRewardDistributorEpochCoinInfo {
    pub fn new(
        clawback_inner_puzzle_hash: Bytes32,
        reward_asset_id: Bytes32,
        reward_distributor_launcher_id: Bytes32,
        reward_distributor_first_epoch_start: u64,
        reward_distributor_epoch_seconds: u64,
    ) -> Result<Self> {
        Ok(Self {
            clawback_inner_puzzle_hash,
            reward_asset_id,
            reward_distributor_launcher_id,
            reward_distributor_first_epoch_start,
            reward_distributor_epoch_seconds,
        })
    }

    pub fn from_constants(
        constants: RewardDistributorConstants,
        reward_distributor_first_epoch_start: u64,
        clawback_inner_puzzle_hash: Bytes32,
    ) -> Result<Self> {
        let reward_asset_id = match constants.reward_distributor_type {
            SdkRewardDistributorType::Cat { asset_id, .. } => asset_id,
            _ => constants.reserve_asset_id,
        };

        Self::new(
            clawback_inner_puzzle_hash,
            reward_asset_id,
            constants.launcher_id,
            reward_distributor_first_epoch_start,
            constants.epoch_seconds,
        )
    }

    pub fn clawback_puzzle_hash(&self, coin_id: Bytes32) -> Result<Bytes32> {
        use chia_sdk_types::puzzles::NonceWrapperArgs;

        Ok(NonceWrapperArgs::<Bytes32, TreeHash> {
            nonce: coin_id,
            inner_puzzle: self.clawback_inner_puzzle_hash.into(),
        }
        .curry_tree_hash()
        .into())
    }

    pub fn inner_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.args().curry_tree_hash().into())
    }

    pub fn puzzle_hash(&self) -> Result<Bytes32> {
        Ok(CatArgs::curry_tree_hash(
            self.reward_asset_id,
            TreeHash::from(self.inner_puzzle_hash()?),
        )
        .into())
    }

    pub(crate) fn args(&self) -> P2NextRewardDistributorEpochArgs {
        P2NextRewardDistributorEpochArgs::new(
            self.clawback_inner_puzzle_hash,
            SingletonStruct::new(self.reward_distributor_launcher_id).tree_hash(),
            self.reward_distributor_first_epoch_start,
            self.reward_distributor_epoch_seconds,
        )
    }
}
