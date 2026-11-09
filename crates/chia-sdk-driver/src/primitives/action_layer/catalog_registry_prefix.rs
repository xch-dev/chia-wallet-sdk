use chia_protocol::Bytes32;
use clvm_traits::clvm_tuple;
use clvm_utils::ToTreeHash;

use crate::prefix_hash;

/// Prefixes of the puzzle announcements that the [`CatalogRegistry`](crate::CatalogRegistry) creates.
///
/// Each announcement is one prefix byte followed by a 32-byte tree hash. Coins that pay for an
/// action assert these announcements to make sure the action happened as expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CatalogRegistryCreatedAnnouncementPrefix {
    /// Announced by the refund action (`$`).
    Refund = b'$',
    /// Announced by the register action (`r`).
    Register = b'r',
}

impl CatalogRegistryCreatedAnnouncementPrefix {
    /// Returns the refund announcement for a precommitment with the given TAIL hash and initial NFT owner puzzle hash.
    pub fn refund(tail_hash: Bytes32, initial_nft_owner_ph: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::Refund as u8,
            clvm_tuple!(tail_hash, initial_nft_owner_ph).tree_hash(),
        )
    }

    /// Returns the register announcement for a CAT with the given TAIL hash and initial NFT owner puzzle hash.
    pub fn register(tail_hash: Bytes32, initial_nft_owner_ph: Bytes32) -> Vec<u8> {
        prefix_hash(
            Self::Register as u8,
            clvm_tuple!(tail_hash, initial_nft_owner_ph).tree_hash(),
        )
    }
}
