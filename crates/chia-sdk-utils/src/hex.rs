use chia_protocol::Bytes;
use hex::FromHexError;

/// Decodes a hex string into [`Bytes`], ignoring a leading `0x` prefix if present.
///
/// Only a lowercase `0x` prefix is stripped. The remaining string must have an even
/// number of hex digits, in either case.
///
/// # Example
///
/// ```
/// use chia_sdk_utils::parse_hex;
///
/// assert_eq!(parse_hex("0xcafe")?.as_ref(), &[0xca, 0xfe]);
/// assert_eq!(parse_hex("CAFE")?.as_ref(), &[0xca, 0xfe]);
/// assert!(parse_hex("0xabc").is_err());
/// # Ok::<(), hex::FromHexError>(())
/// ```
pub fn parse_hex(mut hex: &str) -> Result<Bytes, FromHexError> {
    if let Some(stripped) = hex.strip_prefix("0x") {
        hex = stripped;
    }

    Ok(hex::decode(hex)?.into())
}
