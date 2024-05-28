use bech32::{u5, Variant};
use thiserror::Error;

/// Errors you can get while trying to decode an offer.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DecodeOfferError {
    /// The wrong HRP prefix was used.
    #[error("invalid prefix `{0}`")]
    InvalidPrefix(String),

    /// The address was encoded as bech32, rather than bech32m.
    #[error("encoding is not bech32m")]
    InvalidFormat,

    /// An error occured while trying to decode the address.
    #[error("error when decoding address: {0}")]
    Decode(#[from] bech32::Error),
}

/// Decodes an offer into bytes.
pub fn decode_offer(offer: &str) -> Result<Vec<u8>, DecodeOfferError> {
    let (hrp, data, variant) = bech32::decode(offer)?;

    if variant != Variant::Bech32m {
        return Err(DecodeOfferError::InvalidFormat);
    }

    if hrp.as_str() != "offer" {
        return Err(DecodeOfferError::InvalidPrefix(hrp));
    }

    Ok(bech32::convert_bits(&data, 5, 8, false)?)
}

/// Encodes an offer.
pub fn encode_offer(offer: &[u8]) -> Result<String, bech32::Error> {
    let data = bech32::convert_bits(offer, 8, 5, true)?
        .into_iter()
        .map(u5::try_from_u8)
        .collect::<Result<Vec<_>, bech32::Error>>()?;
    bech32::encode("offer1", data, Variant::Bech32m)
}
