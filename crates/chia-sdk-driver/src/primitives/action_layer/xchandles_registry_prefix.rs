use chia_protocol::Bytes32;
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::prefix_hash;

/// Prefixes of the puzzle announcements that the [`XchandlesRegistry`](crate::XchandlesRegistry) creates.
///
/// Each announcement is one prefix byte followed by a 32-byte hash. Coins that pay for an action
/// assert these announcements to make sure the action happened as expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XchandlesRegistryCreatedAnnouncementPrefix {
    /// Announced by the expire action (`x`).
    Expire = b'x',
    /// Announced by the extend action (`e`).
    Extend = b'e',
    /// Announced by the oracle action (`o`).
    Oracle = b'o',
    /// Announced by the refund action (`$`).
    Refund = b'$',
    /// Announced by the register action (`r`).
    Register = b'r',
}

/// Prefixes of the messages that the [`XchandlesRegistry`](crate::XchandlesRegistry) receives.
///
/// Each message is one prefix byte followed by a 32-byte hash. Most are sent by the owner or
/// resolved NFT singletons of a handle to approve an action. [`Self::UpdateState`] is shared with
/// the [`CatalogRegistry`](crate::CatalogRegistry) and the [`StateScheduler`](crate::StateScheduler),
/// and is sent by a registry's price singleton.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::XchandlesRegistryReceivedMessagePrefix;
/// use clvm_utils::ToTreeHash;
///
/// let state_hash = Bytes32::new([1; 32]).tree_hash();
/// let message = XchandlesRegistryReceivedMessagePrefix::update_state(state_hash);
///
/// assert_eq!(message.len(), 33);
/// assert_eq!(message[0], b's');
/// assert_eq!(&message[1..], state_hash.as_ref());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XchandlesRegistryReceivedMessagePrefix {
    /// Sent by the price singleton to set a new state (`s`).
    UpdateState = b's',
    /// Sent by the new owner NFT to approve a registration (`a`).
    RegisterOwner = b'a',
    /// Sent by the new resolved NFT to approve a registration (`b`).
    RegisterResolved = b'b',
    /// Sent by the new owner NFT to approve re-registering an expired handle (`e`).
    ExpireOwner = b'e',
    /// Sent by the new resolved NFT to approve re-registering an expired handle (`f`).
    ExpireResolved = b'f',
    /// Sent by the current owner NFT to start an update (`i`).
    InitiateUpdate = b'i',
    /// Sent by the old owner NFT to approve executing an update (`u`).
    ExecuteUpdateOldOwner = b'u',
    /// Sent by the new owner NFT to approve executing an update (`o`).
    ExecuteUpdateNewOwner = b'o',
    /// Sent by the new resolved NFT to approve executing an update (`r`).
    ExecuteUpdateNewResolved = b'r',
}

impl XchandlesRegistryReceivedMessagePrefix {
    /// Returns the message that sets the registry state to the state with tree hash `state_hash`.
    pub fn update_state(state_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::UpdateState as u8, state_hash)
    }

    /// Returns the owner approval message for registering with the given precommit coin.
    pub fn register_owner(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::RegisterOwner as u8, precommit_coin_puzzle_hash.into())
    }

    /// Returns the resolved approval message for registering with the given precommit coin.
    pub fn register_resolved(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::RegisterResolved as u8,
            precommit_coin_puzzle_hash.into(),
        )
    }

    /// Returns the owner approval message for re-registering an expired handle with the given precommit coin.
    pub fn expire_owner(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::ExpireOwner as u8, precommit_coin_puzzle_hash.into())
    }

    /// Returns the resolved approval message for re-registering an expired handle with the given precommit coin.
    pub fn expire_resolved(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::ExpireResolved as u8,
            precommit_coin_puzzle_hash.into(),
        )
    }

    /// Returns the message that starts an update, given the tree hash of the update slot value it creates.
    pub fn initiate_update(update_slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::InitiateUpdate as u8, update_slot_value_hash)
    }

    /// Returns the old owner's approval message for executing the update with the given update slot value hash.
    pub fn execute_update_old_owner(update_slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::ExecuteUpdateOldOwner as u8, update_slot_value_hash)
    }

    /// Returns the new owner's approval message for executing the update with the given update slot value hash.
    pub fn execute_update_new_owner(update_slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::ExecuteUpdateNewOwner as u8, update_slot_value_hash)
    }

    /// Returns the new resolved NFT's approval message for executing the update with the given update slot value hash.
    pub fn execute_update_new_resolved(update_slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::ExecuteUpdateNewResolved as u8, update_slot_value_hash)
    }
}

impl XchandlesRegistryCreatedAnnouncementPrefix {
    /// Returns the announcement for expiring a handle and re-registering it with the given precommit coin.
    pub fn expire(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Expire as u8, precommit_coin_puzzle_hash.into())
    }

    /// Returns the announcement for extending `handle` by paying `total_price`.
    pub fn extend(total_price: u64, handle: &str) -> Vec<u8> {
        prefix_hash(
            Self::Extend as u8,
            clvm_tuple!(total_price, handle).tree_hash(),
        )
    }

    /// Returns the announcement that reveals the handle slot value with the given tree hash.
    pub fn oracle(slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::Oracle as u8, slot_value_hash)
    }

    /// Returns the announcement for refunding the given precommit coin.
    pub fn refund(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Refund as u8, precommit_coin_puzzle_hash.into())
    }

    /// Returns the announcement for registering a handle with the given precommit coin.
    pub fn register(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Register as u8, precommit_coin_puzzle_hash.into())
    }
}
