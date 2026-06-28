use bech32::{Variant, u5};
use chia_protocol::{Bytes, Bytes32};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Bech32Error {
    #[error("not bech32m encoded")]
    InvalidFormat,

    #[error("expected 32 bytes, found {0}")]
    WrongLength(usize),

    #[error("expected prefix {1}, found {0}")]
    WrongPrefix(String, String),

    #[error("bech32 error: {0}")]
    Decode(#[from] bech32::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bech32 {
    pub data: Bytes,
    pub prefix: String,
}

impl Bech32 {
    pub fn new(data: Bytes, prefix: String) -> Self {
        Self { data, prefix }
    }

    pub fn decode(address: &str) -> Result<Self, Bech32Error> {
        let (hrp, data, variant) = bech32::decode(address)?;

        if variant != Variant::Bech32m {
            return Err(Bech32Error::InvalidFormat);
        }

        let data = bech32::convert_bits(&data, 5, 8, false)?;

        Ok(Self {
            data: data.into(),
            prefix: hrp,
        })
    }

    pub fn encode(&self) -> Result<String, Bech32Error> {
        let data = bech32::convert_bits(&self.data, 8, 5, true)
            .unwrap()
            .into_iter()
            .map(u5::try_from_u8)
            .collect::<Result<Vec<_>, bech32::Error>>()?;
        Ok(bech32::encode(&self.prefix, data, Variant::Bech32m)?)
    }

    pub fn expect_prefix(self, prefix: &str) -> Result<Bytes, Bech32Error> {
        if self.prefix != prefix {
            return Err(Bech32Error::WrongPrefix(self.prefix, prefix.to_string()));
        }
        Ok(self.data)
    }
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

    pub fn decode(address: &str) -> Result<Self, Bech32Error> {
        Bech32::decode(address).and_then(TryInto::try_into)
    }

    pub fn encode(&self) -> Result<String, Bech32Error> {
        Bech32::from(self.clone()).encode()
    }

    pub fn expect_prefix(self, prefix: &str) -> Result<Bytes32, Bech32Error> {
        if self.prefix != prefix {
            return Err(Bech32Error::WrongPrefix(self.prefix, prefix.to_string()));
        }
        Ok(self.puzzle_hash)
    }
}

impl TryFrom<Bech32> for Address {
    type Error = Bech32Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        let len = bech32.data.len();
        Ok(Self::new(
            bech32
                .data
                .try_into()
                .map_err(|_| Bech32Error::WrongLength(len))?,
            bech32.prefix,
        ))
    }
}

impl From<Address> for Bech32 {
    fn from(address: Address) -> Self {
        Bech32::new(address.puzzle_hash.into(), address.prefix)
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
            Err(Bech32Error::Decode(bech32::Error::MissingSeparator))
        );
        assert_eq!(
            Address::decode("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"),
            Err(Bech32Error::InvalidFormat)
        );
    }
}
