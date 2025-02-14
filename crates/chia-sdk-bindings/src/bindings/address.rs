pub use chia_sdk_utils::AddressInfo;

use chia_protocol::Bytes32;

use crate::Result;

pub fn encode_address(puzzle_hash: Bytes32, prefix: String) -> Result<String> {
    Ok(chia_sdk_utils::encode_address(puzzle_hash, &prefix)?)
}

pub fn decode_address(address: String) -> Result<AddressInfo> {
    Ok(chia_sdk_utils::decode_address(&address)?)
}
