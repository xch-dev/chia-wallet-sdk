use chia_protocol::{Bytes, Bytes32};
use clvm_traits::clvm_tuple;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::prefix_hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XchandlesRegistryCreatedAnnouncementPrefix {
    Expire = b'x',
    Extend = b'e',
    Oracle = b'o',
    Refund = b'$',
    Register = b'r',
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum XchandlesRegistryReceivedMessagePrefix {
    UpdateState = b's',
    Update = b'u',
}

impl XchandlesRegistryReceivedMessagePrefix {
    pub fn update_state(state_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::UpdateState as u8, state_hash)
    }

    pub fn update_handle(
        handle_hash: Bytes32,
        new_owner_launcher_id: Bytes32,
        new_resolved_data: &Bytes,
    ) -> Vec<u8> {
        prefix_hash(
            Self::Update as u8,
            clvm_tuple!(
                handle_hash,
                clvm_tuple!(new_owner_launcher_id, new_resolved_data)
            )
            .tree_hash(),
        )
    }
}

impl XchandlesRegistryCreatedAnnouncementPrefix {
    pub fn expire(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Expire as u8, precommit_coin_puzzle_hash.into())
    }

    pub fn extend(total_price: u64, handle: &str) -> Vec<u8> {
        prefix_hash(
            Self::Extend as u8,
            clvm_tuple!(total_price, handle).tree_hash(),
        )
    }

    pub fn oracle(slot_value_hash: TreeHash) -> Vec<u8> {
        prefix_hash(Self::Oracle as u8, slot_value_hash)
    }

    pub fn refund(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Refund as u8, precommit_coin_puzzle_hash.into())
    }

    pub fn register(precommit_coin_puzzle_hash: Bytes32) -> Vec<u8> {
        prefix_hash(Self::Register as u8, precommit_coin_puzzle_hash.into())
    }
}
