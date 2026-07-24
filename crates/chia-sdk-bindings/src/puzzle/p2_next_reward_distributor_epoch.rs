use bindy::Result;
use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, singleton::SingletonStruct};
use chia_sdk_types::{Mod, puzzles::P2NextRewardDistributorEpochArgs};
use clvm_utils::{ToTreeHash, TreeHash};

pub fn p2_next_reward_distributor_epoch_inner_puzzle_hash(
    clawback_inner_puzzle_hash: Bytes32,
    reward_distributor_launcher_id: Bytes32,
    reward_distributor_first_epoch_start: u64,
    reward_distributor_epoch_seconds: u64,
) -> Result<Bytes32> {
    Ok(P2NextRewardDistributorEpochArgs::new(
        clawback_inner_puzzle_hash,
        SingletonStruct::new(reward_distributor_launcher_id).tree_hash(),
        reward_distributor_first_epoch_start,
        reward_distributor_epoch_seconds,
    )
    .curry_tree_hash()
    .into())
}

pub fn p2_next_reward_distributor_epoch_puzzle_hash(
    reward_cat_asset_id: Bytes32,
    clawback_inner_puzzle_hash: Bytes32,
    reward_distributor_launcher_id: Bytes32,
    reward_distributor_first_epoch_start: u64,
    reward_distributor_epoch_seconds: u64,
) -> Result<Bytes32> {
    let inner_puzzle_hash = p2_next_reward_distributor_epoch_inner_puzzle_hash(
        clawback_inner_puzzle_hash,
        reward_distributor_launcher_id,
        reward_distributor_first_epoch_start,
        reward_distributor_epoch_seconds,
    )?;
    Ok(CatArgs::curry_tree_hash(reward_cat_asset_id, TreeHash::from(inner_puzzle_hash)).into())
}

pub fn p2_next_reward_distributor_epoch_clawback_puzzle_hash(
    coin_id: Bytes32,
    clawback_inner_puzzle_hash: Bytes32,
) -> Result<Bytes32> {
    use chia_sdk_types::puzzles::NonceWrapperArgs;

    Ok(NonceWrapperArgs::<Bytes32, TreeHash> {
        nonce: coin_id,
        inner_puzzle: clawback_inner_puzzle_hash.into(),
    }
    .curry_tree_hash()
    .into())
}
