use bech32::{
    Checksum, DecodeError, EncodeError, Fe32, Fe1024, Hrp,
    primitives::{
        decode::{ChecksumError, UncheckedHrpstring},
        hrp,
    },
};
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

    #[error("bech32 decode error: {0}")]
    Decode(#[from] DecodeError),

    #[error("bech32 encode error: {0}")]
    Encode(#[from] EncodeError),

    #[error("invalid bech32 prefix: {0}")]
    Prefix(#[from] hrp::Error),
}

/// The bech32m checksum, but without the 1023 character length limit, since
/// some bech32m encoded strings (such as offers) can be much longer than that.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Bech32mUnbounded {}

impl Checksum for Bech32mUnbounded {
    type MidstateRepr = u32;

    type CorrectionField = Fe1024;
    const ROOT_GENERATOR: Self::CorrectionField = Fe1024::new([Fe32::P, Fe32::X]);
    const ROOT_EXPONENTS: core::ops::RangeInclusive<usize> = 24..=26;

    const CODE_LENGTH: usize = usize::MAX;
    const CHECKSUM_LENGTH: usize = 6;
    const GENERATOR_SH: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    const TARGET_RESIDUE: u32 = 0x2bc8_30a3;
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
        let unchecked = UncheckedHrpstring::new(address).map_err(DecodeError::Parse)?;

        match unchecked.validate_checksum::<Bech32mUnbounded>() {
            Ok(()) => {}
            Err(ChecksumError::InvalidResidue(error)) if error.matches_bech32_checksum() => {
                return Err(Bech32Error::InvalidFormat);
            }
            Err(error) => return Err(DecodeError::Checksum(error).into()),
        }

        let checked = unchecked.remove_checksum::<Bech32mUnbounded>();

        Ok(Self {
            data: checked.byte_iter().collect::<Vec<u8>>().into(),
            prefix: checked.hrp().to_lowercase(),
        })
    }

    pub fn encode(&self) -> Result<String, Bech32Error> {
        let hrp = Hrp::parse(&self.prefix)?;
        Ok(bech32::encode::<Bech32mUnbounded>(hrp, &self.data)?)
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
    use bech32::primitives::decode::{CharError, UncheckedHrpstringError};

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
    fn test_long_bech32m() {
        let bech32 = Bech32::new(Bytes::new(vec![42; 2000]), "offer".to_string());
        let encoded = bech32.encode().unwrap();
        assert!(encoded.len() > 1023);
        assert_eq!(Bech32::decode(&encoded).unwrap(), bech32);
    }

    #[test]
    fn test_invalid_addresses() {
        assert_eq!(
            Address::decode("hello there!"),
            Err(Bech32Error::Decode(DecodeError::Parse(
                UncheckedHrpstringError::Char(CharError::InvalidChar('!'))
            )))
        );
        assert_eq!(
            Address::decode("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"),
            Err(Bech32Error::InvalidFormat)
        );
    }
}
