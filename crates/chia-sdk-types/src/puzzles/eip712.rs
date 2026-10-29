use chia_protocol::Bytes32;
use sha3::{Digest, Keccak256};

use super::Eip712PrefixAndDomainSeparator;

/// Compute the CHIP-0037 EIP-712 domain separator for the given Chia network.
///
/// Domain matches the schema mandated by CHIP-0037:
/// `{ name: "Chia Coin Spend", version: "1", salt: <genesis_challenge> }`.
pub fn eip712_domain_separator(genesis_challenge: Bytes32) -> Bytes32 {
    let type_hash = Keccak256::digest(b"EIP712Domain(string name,string version,bytes32 salt)");

    let mut to_hash = Vec::with_capacity(128);
    to_hash.extend_from_slice(&type_hash);
    to_hash.extend_from_slice(&Keccak256::digest(b"Chia Coin Spend"));
    to_hash.extend_from_slice(&Keccak256::digest(b"1"));
    to_hash.extend_from_slice(&genesis_challenge);

    Bytes32::new(Keccak256::digest(&to_hash).into())
}

/// Compute the 34-byte concatenation of the EIP-712 prefix (`\x19\x01`) and
/// the domain separator for the given Chia network.
pub fn eip712_prefix_and_domain_separator(
    genesis_challenge: Bytes32,
) -> Eip712PrefixAndDomainSeparator {
    let mut bytes = [0u8; 34];
    bytes[0] = 0x19;
    bytes[1] = 0x01;
    bytes[2..].copy_from_slice(&eip712_domain_separator(genesis_challenge));
    bytes.into()
}

/// Keccak-256 hash of the canonical CHIP-0037 EIP-712 type signature,
/// `ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)`.
pub fn eip712_type_hash() -> Bytes32 {
    Bytes32::new(
        Keccak256::digest(b"ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)").into(),
    )
}

/// Compute the 32-byte EIP-712 digest the off-chain wallet must sign.
///
/// `digest = keccak256(prefix_and_domain || keccak256(type_hash || coin_id ||
/// delegated_puzzle_hash))`
pub fn eip712_hash_to_sign(
    prefix_and_domain_separator: &Eip712PrefixAndDomainSeparator,
    coin_id: Bytes32,
    delegated_puzzle_hash: Bytes32,
) -> Bytes32 {
    let mut to_hash = Vec::with_capacity(96);
    to_hash.extend_from_slice(&eip712_type_hash());
    to_hash.extend_from_slice(&coin_id);
    to_hash.extend_from_slice(&delegated_puzzle_hash);
    let message_hash = Keccak256::digest(&to_hash);

    let mut to_hash = Vec::with_capacity(34 + 32);
    to_hash.extend_from_slice(prefix_and_domain_separator);
    to_hash.extend_from_slice(&message_hash);

    Bytes32::new(Keccak256::digest(&to_hash).into())
}
