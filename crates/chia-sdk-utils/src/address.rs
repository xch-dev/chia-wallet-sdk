use bech32::{u5, Variant};
use chia_protocol::Bytes32;
use hex::FromHexError;
use thiserror::Error;

/// Errors you can get while trying to decode an address.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    /// The address was encoded as bech32, rather than bech32m.
    #[error("encoding is not bech32m")]
    InvalidFormat,

    /// The data was not 32 bytes in length.
    #[error("wrong length, expected 32 bytes but found {0}")]
    WrongLength(usize),

    /// An error occured while trying to decode the address.
    #[error("error when decoding address: {0}")]
    Decode(#[from] bech32::Error),
}

/// Errors you can get while trying to decode a puzzle hash.
#[derive(Error, Debug, Clone, Copy, PartialEq)]
pub enum PuzzleHashError {
    /// The buffer was not 32 bytes in length.
    #[error("wrong length, expected 32 bytes but found {0}")]
    WrongLength(usize),

    /// An error occured while trying to decode the puzzle hash.
    #[error("error when decoding puzzle hash: {0}")]
    Decode(#[from] FromHexError),
}

/// Decodes a puzzle hash from hex, with or without a prefix.
pub fn decode_puzzle_hash(puzzle_hash: &str) -> Result<Bytes32, PuzzleHashError> {
    let data = hex::decode(strip_prefix(puzzle_hash))?;
    let length = data.len();
    data.try_into()
        .map_err(|_| PuzzleHashError::WrongLength(length))
}

/// Encodes a puzzle hash into hex, with or without a prefix.
pub fn encode_puzzle_hash(puzzle_hash: Bytes32, include_0x: bool) -> String {
    if include_0x {
        format!("0x{}", hex::encode(puzzle_hash))
    } else {
        hex::encode(puzzle_hash)
    }
}

/// Decodes an address into a puzzle hash and HRP prefix.
pub fn decode_address(address: &str) -> Result<(Bytes32, String), AddressError> {
    let (hrp, data, variant) = bech32::decode(address)?;

    if variant != Variant::Bech32m {
        return Err(AddressError::InvalidFormat);
    }

    let data = bech32::convert_bits(&data, 5, 8, false)?;
    let length = data.len();
    let puzzle_hash = data
        .try_into()
        .map_err(|_| AddressError::WrongLength(length))?;

    Ok((puzzle_hash, hrp))
}

/// Encodes an address with a given HRP prefix.
pub fn encode_address(puzzle_hash: Bytes32, prefix: &str) -> Result<String, bech32::Error> {
    let data = bech32::convert_bits(&puzzle_hash, 8, 5, true)
        .unwrap()
        .into_iter()
        .map(u5::try_from_u8)
        .collect::<Result<Vec<_>, bech32::Error>>()?;
    bech32::encode(prefix, data, Variant::Bech32m)
}

/// Removes the `0x` prefix from a puzzle hash in hex format.
pub fn strip_prefix(puzzle_hash: &str) -> &str {
    if let Some(puzzle_hash) = puzzle_hash.strip_prefix("0x") {
        puzzle_hash
    } else if let Some(puzzle_hash) = puzzle_hash.strip_prefix("0X") {
        puzzle_hash
    } else {
        puzzle_hash
    }
}

#[cfg(test)]
mod tests {
    use hex_literal::hex;

    use super::*;

    fn check_ph(expected: &str) {
        let expected = strip_prefix(expected);
        let puzzle_hash = decode_puzzle_hash(expected).unwrap();
        let actual = encode_puzzle_hash(puzzle_hash, false);
        assert_eq!(actual, expected);
    }

    fn check_addr(expected: &str) {
        let (puzzle_hash, prefix) = decode_address(expected).unwrap();
        let actual = encode_address(puzzle_hash, &prefix).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_prefix() {
        check_ph("0x2999682870bd24e7fd0ef6324c69794ff93fc41b016777d2edd5ea8575bdaa31");
        check_ph("0x99619cc6888f1bd30acd6e8c1f4065dafeba2246bfc3465cddda4e6656083791");
        check_ph("0X7cc6494dd96d32c97b5f6ba77caae269acd6c86593ada66f343050ce709e904a");
        check_ph("0X9f057817ad576b24ec60a25ded08f5bde6db0aa0beeb0c099e3ce176866e1c4b");
    }

    #[test]
    fn test_puzzle_hashes() {
        check_ph(&hex::encode([0; 32]));
        check_ph(&hex::encode([255; 32]));
        check_ph(&hex::encode([127; 32]));
        check_ph(&hex::encode([1; 32]));
        check_ph("f46ec440aeb9b3baa19968810a8537ec4ff406c09c994dd7d3222b87258a52ff");
        check_ph("2f981b2f9510ef9e62523e6b38fc933e2f060c411cfa64906413ddfd56be8dc1");
        check_ph("3e09bdd6b19659555a7c8456c5af54d004d774f3d44689360d4778ce685201ad");
        check_ph("d16c2ad7c5642532659e424dc0d7e4a85779c6dab801b5e6117a8c8587156472");
    }

    #[test]
    fn test_invalid_puzzle_hashes() {
        assert_eq!(
            decode_puzzle_hash("ac4fd55996a1186fffc30c5b60385a88fd78d538f1c9febbfa9c8a9e9a170ad"),
            Err(PuzzleHashError::Decode(FromHexError::OddLength))
        );
        assert_eq!(
            decode_puzzle_hash(&hex::encode(hex!(
                "
            dfe399911acc4426f44bf31f4d817f6b69f244bbad138a28
            25c05550f7d2ab70c35408f764281febd624ac8cdfc91817
            "
            ))),
            Err(PuzzleHashError::WrongLength(48))
        );
        assert_eq!(
            decode_puzzle_hash("hello there!"),
            Err(PuzzleHashError::Decode(FromHexError::InvalidHexCharacter {
                c: 'h',
                index: 0
            }))
        );
    }

    #[test]
    fn test_addresses() {
        check_addr("xch1a0t57qn6uhe7tzjlxlhwy2qgmuxvvft8gnfzmg5detg0q9f3yc3s2apz0h");
        check_addr("xch1ftxk2v033kv94ueucp0a34sgt9398vle7l7g3q9k4leedjmmdysqvv6q96");
        check_addr("xch1ay273ctc9c6nxmzmzsup28scrce8ney84j4nlewdlaxqs22v53ksxgf38f");
        check_addr("xch1avnwmy2fuesq7h2jnxehlrs9msrad9uuvrhms35k2pqwmjv56y5qk7zm6v");
    }

    #[test]
    fn test_invalid_addresses() {
        assert_eq!(
            decode_address("hello there!"),
            Err(AddressError::Decode(bech32::Error::MissingSeparator))
        );
        assert_eq!(
            decode_address("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"),
            Err(AddressError::InvalidFormat)
        );
    }
}
