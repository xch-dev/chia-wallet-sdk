use bech32::{u5, Variant};
use chia_protocol::Bytes32;
use thiserror::Error;

#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    #[error("encoding is not bech32m")]
    InvalidFormat,

    #[error("wrong length, expected 32 bytes but found {0}")]
    WrongLength(usize),

    #[error("error when decoding address: {0}")]
    Decode(#[from] bech32::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub puzzle_hash: Bytes32,
    pub prefix: String,
}

impl Address {
    pub fn new(puzzle_hash: Bytes32, prefix: String) -> Self {
        Self {
            puzzle_hash,
            prefix,
        }
    }

    pub fn decode(address: &str) -> Result<Self, AddressError> {
        let (hrp, data, variant) = bech32::decode(address)?;

        if variant != Variant::Bech32m {
            return Err(AddressError::InvalidFormat);
        }

        let data = bech32::convert_bits(&data, 5, 8, false)?;
        let length = data.len();
        let puzzle_hash = data
            .try_into()
            .map_err(|_| AddressError::WrongLength(length))?;

        Ok(Self {
            puzzle_hash,
            prefix: hrp,
        })
    }

    pub fn encode(&self) -> Result<String, AddressError> {
        let data = bech32::convert_bits(&self.puzzle_hash, 8, 5, true)
            .unwrap()
            .into_iter()
            .map(u5::try_from_u8)
            .collect::<Result<Vec<_>, bech32::Error>>()?;
        Ok(bech32::encode(&self.prefix, data, Variant::Bech32m)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_addr(expected: &str) {
        let info = Address::decode(expected).unwrap();
        let actual = info.encode().unwrap();
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
            Address::decode("hello there!"),
            Err(AddressError::Decode(bech32::Error::MissingSeparator))
        );
        assert_eq!(
            Address::decode("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"),
            Err(AddressError::InvalidFormat)
        );
    }
}
