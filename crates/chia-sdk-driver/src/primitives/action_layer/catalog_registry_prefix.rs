use chia_protocol::Bytes32;
use clvm_traits::clvm_tuple;
use clvm_utils::ToTreeHash;

use crate::prefix_hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CatalogRegistryCreatedAnnouncementPrefix {
    Refund = b'$',
    Register = b'r',
}

impl CatalogRegistryCreatedAnnouncementPrefix {
    pub fn refund(tail_hash: Bytes32, initial_nft_owner_ph: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::Refund as u8,
            clvm_tuple!(tail_hash, initial_nft_owner_ph).tree_hash(),
        )
    }

    pub fn register(tail_hash: Bytes32, initial_nft_owner_ph: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::Register as u8,
            clvm_tuple!(tail_hash, initial_nft_owner_ph).tree_hash(),
        )
    }
}
