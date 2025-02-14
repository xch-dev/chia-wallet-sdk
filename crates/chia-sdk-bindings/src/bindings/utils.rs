use chia_protocol::{Bytes, Bytes32};

pub use chia_sdk_utils::AddressInfo;

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
