use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{
    Mod,
    puzzles::{
        AddDelegatedPuzzleWrapper, Force1of2RestrictedVariable, ForceSingletonRecreationMod,
        PreventConditionOpcode, PreventMultipleCreateCoinsMod, Timelock,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};

/// The message that a vault custody member which commits to the vault's puzzle hash signs, which is the delegated
/// puzzle hash followed by the vault coin's full puzzle hash.
///
/// This is the full `AGG_SIG_PUZZLE` message of a [`BlsMemberPuzzleAssert`](chia_sdk_types::puzzles::BlsMemberPuzzleAssert).
/// The secp256k1 and secp256r1 puzzle assert members sign the SHA-256 hash of it instead. Since it doesn't commit to
/// the coin id, the signature can be replayed on any vault coin with the same puzzle hash.
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

/// The message that a vault custody member which commits to the vault's coin id signs, which is the delegated puzzle
/// hash followed by the vault coin id and the network's genesis challenge.
///
/// This is the full `AGG_SIG_ME` message of a [`BlsMember`](chia_sdk_types::puzzles::BlsMember), whose additional
/// data is the genesis challenge. The secp256k1 and secp256r1 members sign the SHA-256 hash of the delegated puzzle
/// hash and coin id instead, without the genesis challenge.
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

/// The version of a vault's puzzle, which determines the restrictions on its recovery path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VaultVersion {
    /// The original vault format.
    Legacy,
    /// Adds a [`ForceSingletonRecreationMod`] wrapper to the end of the recovery path's restrictions, so that
    /// starting recovery must recreate the vault singleton rather than melting it or changing its amount.
    ForceSingletonRecreation,
}

/// Calculates the delegated puzzle hash that a recovery key signs when starting recovery, which is the
/// delegated puzzle hash wrapped in each of the recovery path's delegated puzzle wrappers.
///
/// This can be passed to [`calculate_vault_coin_message`] or [`calculate_vault_puzzle_message`], depending
/// on which message format the recovery key signs.
pub fn vault_recovery_delegated_puzzle_hash(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    version: VaultVersion,
) -> Bytes32 {
    let mut delegated_puzzle_hash: TreeHash = delegated_puzzle_hash.into();

    let mut restrictions = vec![
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

    match version {
        VaultVersion::Legacy => {}
        VaultVersion::ForceSingletonRecreation => {
            restrictions.push(ForceSingletonRecreationMod::mod_hash());
        }
    }

    for restriction in restrictions.into_iter().rev() {
        delegated_puzzle_hash =
            AddDelegatedPuzzleWrapper::new(restriction, delegated_puzzle_hash).curry_tree_hash();
    }

    delegated_puzzle_hash.into()
}

/// The message that a recovery key signs to start recovery of a vault, if it signs the
/// [`calculate_vault_coin_message`] format. This is the coin message of the
/// [`vault_recovery_delegated_puzzle_hash`].
///
/// `left_side_subtree_hash` is the hash of the left side of the vault's 1-of-2 custody, which recovery must keep.
/// The new custody can only change the right side, which is restricted by the `recovery_timelock`.
pub fn calculate_vault_start_recovery_message(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
    version: VaultVersion,
) -> Bytes {
    calculate_vault_coin_message(
        vault_recovery_delegated_puzzle_hash(
            delegated_puzzle_hash,
            left_side_subtree_hash,
            recovery_timelock,
            version,
        ),
        vault_coin_id,
        genesis_challenge,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_force_singleton_recreation_is_innermost_recovery_wrapper() {
        let delegated_puzzle_hash = Bytes32::new([1; 32]);
        let left_side_subtree_hash = Bytes32::new([2; 32]);

        let legacy = vault_recovery_delegated_puzzle_hash(
            delegated_puzzle_hash,
            left_side_subtree_hash,
            100,
            VaultVersion::Legacy,
        );

        let force_singleton_recreation = vault_recovery_delegated_puzzle_hash(
            delegated_puzzle_hash,
            left_side_subtree_hash,
            100,
            VaultVersion::ForceSingletonRecreation,
        );

        assert_ne!(legacy, force_singleton_recreation);

        let wrapped = AddDelegatedPuzzleWrapper::new(
            ForceSingletonRecreationMod::mod_hash(),
            TreeHash::from(delegated_puzzle_hash),
        )
        .curry_tree_hash();

        assert_eq!(
            force_singleton_recreation,
            vault_recovery_delegated_puzzle_hash(
                wrapped.into(),
                left_side_subtree_hash,
                100,
                VaultVersion::Legacy,
            )
        );
    }

    #[test]
    fn test_start_recovery_message_format() {
        let delegated_puzzle_hash = Bytes32::new([1; 32]);
        let left_side_subtree_hash = Bytes32::new([2; 32]);
        let vault_coin_id = Bytes32::new([3; 32]);
        let genesis_challenge = Bytes32::new([4; 32]);

        for version in [VaultVersion::Legacy, VaultVersion::ForceSingletonRecreation] {
            let wrapped = vault_recovery_delegated_puzzle_hash(
                delegated_puzzle_hash,
                left_side_subtree_hash,
                100,
                version,
            );

            assert_eq!(
                calculate_vault_start_recovery_message(
                    delegated_puzzle_hash,
                    left_side_subtree_hash,
                    100,
                    vault_coin_id,
                    genesis_challenge,
                    version,
                ),
                Bytes::from(
                    [
                        wrapped.to_vec(),
                        vault_coin_id.to_vec(),
                        genesis_challenge.to_vec()
                    ]
                    .concat()
                )
            );
        }
    }
}
