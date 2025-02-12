use chia_protocol::Bytes32;
use chia_sdk_bindings_derive::bind;

use crate::Result;

#[bind]
pub fn encode_address(puzzle_hash: Bytes32, prefix: String) -> Result<String> {
    Ok(chia_sdk_utils::encode_address(puzzle_hash, &prefix)?)
}
