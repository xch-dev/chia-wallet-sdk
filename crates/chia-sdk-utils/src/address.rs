use bech32::{u5, Variant};
use chia_protocol::Bytes32;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn check_addr(expected: &str) {
        let (puzzle_hash, prefix) = decode_address(expected).unwrap();
        let actual = encode_address(puzzle_hash, &prefix).unwrap();
        assert_eq!(actual, expected);
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
