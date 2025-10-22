use chia_protocol::Bytes;
use hex::FromHexError;

pub fn parse_hex(mut hex: String) -> Result<Bytes, FromHexError> {
    if let Some(stripped) = hex.strip_prefix("0x") {
        hex = stripped.to_string();
    }

    Ok(hex::decode(hex)?.into())
}
