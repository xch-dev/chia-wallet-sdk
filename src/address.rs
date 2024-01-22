/// Parses an address into a puzzle hash.
pub fn parse_address(address: &str) -> [u8; 32] {
    if let Ok(puzzle_hash) = hex::decode(strip_prefix(address)) {
        puzzle_hash.try_into().expect("invalid puzzle hash")
    } else {
        let (_hrp, data, _variant) = bech32::decode(address).expect("invalid address");
        let puzzle_hash = bech32::convert_bits(&data, 5, 8, false).expect("invalid address data");
        puzzle_hash
            .try_into()
            .expect("invalid address puzzle hash encoding")
    }
}

/// Removes the `0x` prefix from a puzzle hash in hex format.
fn strip_prefix(puzzle_hash: &str) -> &str {
    if let Some(puzzle_hash) = puzzle_hash.strip_prefix("0x") {
        puzzle_hash
    } else if let Some(puzzle_hash) = puzzle_hash.strip_prefix("0X") {
        puzzle_hash
    } else {
        puzzle_hash
    }
}
