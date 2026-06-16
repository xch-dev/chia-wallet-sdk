use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{
    Mod,
    puzzles::{
        AddDelegatedPuzzleWrapper, Force1of2RestrictedVariable, PreventConditionOpcode,
        PreventMultipleCreateCoinsMod, Timelock,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};

pub fn calculate_vault_puzzle_message(
    delegated_puzzle_hash: Bytes32,
    vault_puzzle_hash: Bytes32,
) -> Bytes {
    [
        delegated_puzzle_hash.to_bytes(),
        vault_puzzle_hash.to_bytes(),
    ]
    .concat()
    .into()
}

pub fn calculate_vault_coin_message(
    delegated_puzzle_hash: Bytes32,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Bytes {
    [
        delegated_puzzle_hash.to_bytes(),
        vault_coin_id.to_bytes(),
        genesis_challenge.to_bytes(),
    ]
    .concat()
    .into()
}

pub fn calculate_vault_start_recovery_message(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Bytes {
    let mut delegated_puzzle_hash: TreeHash = delegated_puzzle_hash.into();

    let restrictions = vec![
        Force1of2RestrictedVariable::new(
            left_side_subtree_hash,
            0,
            vec![Timelock::new(recovery_timelock).curry_tree_hash()]
                .tree_hash()
                .into(),
            ().tree_hash().into(),
        )
        .curry_tree_hash(),
        PreventConditionOpcode::new(CREATE_COIN_ANNOUNCEMENT).curry_tree_hash(),
        PreventConditionOpcode::new(CREATE_PUZZLE_ANNOUNCEMENT).curry_tree_hash(),
        PreventConditionOpcode::new(SEND_MESSAGE).curry_tree_hash(),
        PreventConditionOpcode::new(RECEIVE_MESSAGE).curry_tree_hash(),
        PreventMultipleCreateCoinsMod::mod_hash(),
    ];

    for restriction in restrictions.into_iter().rev() {
        delegated_puzzle_hash =
            AddDelegatedPuzzleWrapper::new(restriction, delegated_puzzle_hash).curry_tree_hash();
    }

    [
        delegated_puzzle_hash.to_bytes(),
        vault_coin_id.to_bytes(),
        genesis_challenge.to_bytes(),
    ]
    .concat()
    .into()
}
