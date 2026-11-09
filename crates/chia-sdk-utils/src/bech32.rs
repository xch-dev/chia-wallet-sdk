use bech32::{
    Checksum, DecodeError, EncodeError, Fe32, Fe1024, Hrp,
    primitives::{
        decode::{ChecksumError, UncheckedHrpstring},
        hrp,
    },
};
use chia_protocol::{Bytes, Bytes32};
use thiserror::Error;

/// An error that occurs when encoding or decoding a [`Bech32`] string or an [`Address`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Bech32Error {
    /// The string is valid bech32, but uses the original bech32 checksum instead of bech32m.
    #[error("not bech32m encoded")]
    InvalidFormat,

    /// An [`Address`] must contain exactly 32 bytes of data, but this many were decoded.
    #[error("expected 32 bytes, found {0}")]
    WrongLength(usize),

    /// The prefix didn't match the expected one. The fields are the prefix that was found,
    /// followed by the prefix that was expected.
    #[error("expected prefix {1}, found {0}")]
    WrongPrefix(String, String),

    /// The string couldn't be decoded, for example because of an invalid checksum or character.
    #[error("bech32 decode error: {0}")]
    Decode(#[from] DecodeError),

    /// The data couldn't be encoded.
    #[error("bech32 encode error: {0}")]
    Encode(#[from] EncodeError),

    /// The prefix isn't a valid bech32 human-readable part.
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

/// Arbitrary data encoded as a bech32m string with a human-readable prefix.
///
/// This is used for any bech32m value, regardless of length. For 32 byte puzzle hashes,
/// use [`Address`] instead, which checks the length.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes;
/// use chia_sdk_utils::Bech32;
///
/// let bech32 = Bech32::new(Bytes::new(vec![1, 2, 3]), "test".to_string());
/// let encoded = bech32.encode()?;
/// assert!(encoded.starts_with("test1"));
///
/// let decoded = Bech32::decode(&encoded)?;
/// assert_eq!(decoded, bech32);
/// assert_eq!(decoded.expect_prefix("test")?.as_ref(), &[1, 2, 3]);
/// # Ok::<(), chia_sdk_utils::Bech32Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bech32 {
    /// The decoded data.
    pub data: Bytes,
    /// The human-readable part of the string, before the `1` separator.
    pub prefix: String,
}

impl Bech32 {
    /// Creates a new [`Bech32`] value from its data and prefix.
    pub fn new(data: Bytes, prefix: String) -> Self {
        Self { data, prefix }
    }

    /// Decodes a bech32m string into its data and prefix.
    ///
    /// Returns [`Bech32Error::InvalidFormat`] if the string uses the original bech32
    /// checksum rather than bech32m.
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

    /// Encodes the data as a bech32m string with the prefix.
    ///
    /// Returns an error if the prefix isn't a valid bech32 human-readable part, or if the
    /// resulting string would be too long.
    pub fn encode(&self) -> Result<String, Bech32Error> {
        let hrp = Hrp::parse(&self.prefix)?;
        Ok(bech32::encode::<Bech32mUnbounded>(hrp, &self.data)?)
    }

    /// Returns the data if the prefix matches `prefix`, otherwise returns
    /// [`Bech32Error::WrongPrefix`].
    pub fn expect_prefix(self, prefix: &str) -> Result<Bytes, Bech32Error> {
        if self.prefix != prefix {
            return Err(Bech32Error::WrongPrefix(self.prefix, prefix.to_string()));
        }
        Ok(self.data)
    }
}

/// A Chia address, which is a puzzle hash encoded as a bech32m string.
///
/// The prefix identifies the network, for example `xch` on mainnet and `txch` on testnets.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_utils::Address;
///
/// let address = Address::new(Bytes32::new([0; 32]), "txch".to_string());
/// let encoded = address.encode()?;
/// assert!(encoded.starts_with("txch1"));
///
/// let decoded = Address::decode(&encoded)?;
/// assert_eq!(decoded.expect_prefix("txch")?, Bytes32::new([0; 32]));
/// # Ok::<(), chia_sdk_utils::Bech32Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// The puzzle hash that coins sent to this address are locked with.
    pub puzzle_hash: Bytes32,
    /// The human-readable part of the address, such as `xch` or `txch`.
    pub prefix: String,
}

impl Address {
    /// Creates a new [`Address`] from a puzzle hash and prefix.
    pub fn new(puzzle_hash: Bytes32, prefix: String) -> Self {
        Self {
            puzzle_hash,
            prefix,
        }
    }

    /// Decodes a bech32m address string.
    ///
    /// Returns [`Bech32Error::WrongLength`] if the data isn't exactly 32 bytes long.
    /// The prefix is not checked, so use [`Address::expect_prefix`] to verify the network.
    pub fn decode(address: &str) -> Result<Self, Bech32Error> {
        Bech32::decode(address).and_then(TryInto::try_into)
    }

    /// Encodes the puzzle hash as a bech32m address string with the prefix.
    pub fn encode(&self) -> Result<String, Bech32Error> {
        Bech32::from(self.clone()).encode()
    }

    /// Returns the puzzle hash if the prefix matches `prefix`, otherwise returns
    /// [`Bech32Error::WrongPrefix`].
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
