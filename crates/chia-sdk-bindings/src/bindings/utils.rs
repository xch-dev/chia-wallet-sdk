pub use chia_sdk_utils::AddressInfo;

use chia_protocol::{Bytes, Bytes32};
use sha2::{Digest, Sha256};

use crate::Result;

pub fn from_hex(value: String) -> Result<Bytes> {
    Ok(hex::decode(value)?.into())
}

pub fn to_hex(value: Bytes) -> Result<String> {
    Ok(hex::encode(value))
}

pub fn encode_address(puzzle_hash: Bytes32, prefix: String) -> Result<String> {
    Ok(chia_sdk_utils::encode_address(puzzle_hash, &prefix)?)
}

pub fn decode_address(address: String) -> Result<AddressInfo> {
    Ok(chia_sdk_utils::decode_address(&address)?)
}

pub fn bytes_equal(lhs: Bytes, rhs: Bytes) -> Result<bool> {
    Ok(lhs == rhs)
}

pub fn tree_hash_atom(atom: Bytes32) -> Result<Bytes32> {
    Ok(clvm_utils::tree_hash_atom(&atom).into())
}

pub fn tree_hash_pair(first: Bytes32, rest: Bytes32) -> Result<Bytes32> {
    Ok(clvm_utils::tree_hash_pair(first.into(), rest.into()).into())
}

pub fn sha256(value: Bytes) -> Result<Bytes32> {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let hash: [u8; 32] = hasher.finalize().into();
    Ok(hash.into())
}
