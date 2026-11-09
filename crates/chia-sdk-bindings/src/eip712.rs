use bindy::Result;
use chia_protocol::{Bytes32, BytesImpl};
use chia_sdk_types::{
    Mod,
    puzzles::{self, Eip712Member},
};
use clvm_utils::TreeHash;

use crate::K1PublicKey;

/// Keccak-256 hash of the canonical CHIP-0037 EIP-712 type signature,
/// `ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)`.
pub fn eip712_type_hash() -> Result<Bytes32> {
    Ok(puzzles::eip712_type_hash())
}

/// Compute the CHIP-0037 EIP-712 domain separator for the given Chia network.
pub fn eip712_domain_separator(genesis_challenge: Bytes32) -> Result<Bytes32> {
    Ok(puzzles::eip712_domain_separator(genesis_challenge))
}

/// Compute the 34-byte concatenation of the EIP-712 prefix (`\x19\x01`) and
/// the domain separator for the given Chia network, as curried into
/// `Eip712Member` and `p2_eip712_message`.
pub fn eip712_prefix_and_domain_separator(genesis_challenge: Bytes32) -> Result<BytesImpl<34>> {
    Ok(puzzles::eip712_prefix_and_domain_separator(
        genesis_challenge,
    ))
}

/// Compute the 32-byte EIP-712 digest the off-chain wallet must sign for the
/// given coin and delegated puzzle hash.
pub fn eip712_hash_to_sign(
    genesis_challenge: Bytes32,
    coin_id: Bytes32,
    delegated_puzzle_hash: Bytes32,
) -> Result<Bytes32> {
    Ok(puzzles::eip712_hash_to_sign(
        &puzzles::eip712_prefix_and_domain_separator(genesis_challenge),
        coin_id,
        delegated_puzzle_hash,
    ))
}

/// Compute the curried tree hash of an `Eip712Member`, without any MIPS
/// wrapper layers.
///
/// Use [`eip712_member_hash`](crate::eip712_member_hash) instead to get the
/// hash of the member as it appears in a MIPS tree.
pub fn eip712_member_inner_puzzle_hash(
    genesis_challenge: Bytes32,
    public_key: K1PublicKey,
) -> Result<TreeHash> {
    Ok(Eip712Member::new(
        puzzles::eip712_prefix_and_domain_separator(genesis_challenge),
        puzzles::eip712_type_hash(),
        public_key.0,
    )
    .curry_tree_hash())
}
