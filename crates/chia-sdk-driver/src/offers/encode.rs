use bech32::{u5, Variant};

use crate::DriverError;

pub fn encode_offer_data(offer: &[u8]) -> Result<String, DriverError> {
    let data = bech32::convert_bits(offer, 8, 5, true)?
        .into_iter()
        .map(u5::try_from_u8)
        .collect::<Result<Vec<_>, bech32::Error>>()?;
    Ok(bech32::encode("offer", data, Variant::Bech32m)?)
}

pub fn decode_offer_data(offer: &str) -> Result<Vec<u8>, DriverError> {
    let (hrp, data, variant) = bech32::decode(offer)?;

    if variant != Variant::Bech32m {
        return Err(DriverError::InvalidFormat);
    }

    if hrp.as_str() != "offer" {
        return Err(DriverError::InvalidPrefix(hrp));
    }

    Ok(bech32::convert_bits(&data, 5, 8, false)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_offer_data() {
        let offer = b"hello world";
        let encoded = encode_offer_data(offer).unwrap();
        let decoded = decode_offer_data(&encoded).unwrap();
        assert_eq!(offer, decoded.as_slice());
    }
}
