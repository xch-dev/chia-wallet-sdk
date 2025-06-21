use std::io::Read;

use bech32::{u5, Variant};
use chia_protocol::SpendBundle;
use chia_puzzles::{
    CAT_PUZZLE, NFT_METADATA_UPDATER_DEFAULT, NFT_OWNERSHIP_LAYER,
    NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES, NFT_STATE_LAYER,
    P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE, SETTLEMENT_PAYMENT, SINGLETON_TOP_LAYER_V1_1,
};
use chia_traits::Streamable;
use flate2::{
    read::{ZlibDecoder, ZlibEncoder},
    Compress, Compression, Decompress, FlushDecompress,
};
use hex_literal::hex;
use once_cell::sync::Lazy;

use crate::DriverError;

pub fn compress_offer(spend_bundle: &SpendBundle) -> Result<Vec<u8>, DriverError> {
    compress_offer_bytes(&spend_bundle.to_bytes()?)
}

pub fn decompress_offer(bytes: &[u8]) -> Result<SpendBundle, DriverError> {
    Ok(SpendBundle::from_bytes(&decompress_offer_bytes(bytes)?)?)
}

pub fn encode_offer(spend_bundle: &SpendBundle) -> Result<String, DriverError> {
    encode_offer_data(&compress_offer(spend_bundle)?)
}

pub fn decode_offer(text: &str) -> Result<SpendBundle, DriverError> {
    decompress_offer(&decode_offer_data(text)?)
}

const CAT_PUZZLE_V1: [u8; 1420] = hex!(
    "
    ff02ffff01ff02ff5effff04ff02ffff04ffff04ff05ffff04ffff0bff2cff05
    80ffff04ff0bff80808080ffff04ffff02ff17ff2f80ffff04ff5fffff04ffff
    02ff2effff04ff02ffff04ff17ff80808080ffff04ffff0bff82027fff82057f
    ff820b7f80ffff04ff81bfffff04ff82017fffff04ff8202ffffff04ff8205ff
    ffff04ff820bffff80808080808080808080808080ffff04ffff01ffffffff81
    ca3dff46ff0233ffff3c04ff01ff0181cbffffff02ff02ffff03ff05ffff01ff
    02ff32ffff04ff02ffff04ff0dffff04ffff0bff22ffff0bff2cff3480ffff0b
    ff22ffff0bff22ffff0bff2cff5c80ff0980ffff0bff22ff0bffff0bff2cff80
    80808080ff8080808080ffff010b80ff0180ffff02ffff03ff0bffff01ff02ff
    ff03ffff09ffff02ff2effff04ff02ffff04ff13ff80808080ff820b9f80ffff
    01ff02ff26ffff04ff02ffff04ffff02ff13ffff04ff5fffff04ff17ffff04ff
    2fffff04ff81bfffff04ff82017fffff04ff1bff8080808080808080ffff04ff
    82017fff8080808080ffff01ff088080ff0180ffff01ff02ffff03ff17ffff01
    ff02ffff03ffff20ff81bf80ffff0182017fffff01ff088080ff0180ffff01ff
    088080ff018080ff0180ffff04ffff04ff05ff2780ffff04ffff10ff0bff5780
    ff778080ff02ffff03ff05ffff01ff02ffff03ffff09ffff02ffff03ffff09ff
    11ff7880ffff0159ff8080ff0180ffff01818f80ffff01ff02ff7affff04ff02
    ffff04ff0dffff04ff0bffff04ffff04ff81b9ff82017980ff808080808080ff
    ff01ff02ff5affff04ff02ffff04ffff02ffff03ffff09ff11ff7880ffff01ff
    04ff78ffff04ffff02ff36ffff04ff02ffff04ff13ffff04ff29ffff04ffff0b
    ff2cff5b80ffff04ff2bff80808080808080ff398080ffff01ff02ffff03ffff
    09ff11ff2480ffff01ff04ff24ffff04ffff0bff20ff2980ff398080ffff0109
    80ff018080ff0180ffff04ffff02ffff03ffff09ff11ff7880ffff0159ff8080
    ff0180ffff04ffff02ff7affff04ff02ffff04ff0dffff04ff0bffff04ff17ff
    808080808080ff80808080808080ff0180ffff01ff04ff80ffff04ff80ff1780
    8080ff0180ffffff02ffff03ff05ffff01ff04ff09ffff02ff26ffff04ff02ff
    ff04ff0dffff04ff0bff808080808080ffff010b80ff0180ff0bff22ffff0bff
    2cff5880ffff0bff22ffff0bff22ffff0bff2cff5c80ff0580ffff0bff22ffff
    02ff32ffff04ff02ffff04ff07ffff04ffff0bff2cff2c80ff8080808080ffff
    0bff2cff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff
    02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04ff
    0dff8080808080ffff01ff0bff2cff058080ff0180ffff04ffff04ff28ffff04
    ff5fff808080ffff02ff7effff04ff02ffff04ffff04ffff04ff2fff0580ffff
    04ff5fff82017f8080ffff04ffff02ff7affff04ff02ffff04ff0bffff04ff05
    ffff01ff808080808080ffff04ff17ffff04ff81bfffff04ff82017fffff04ff
    ff0bff8204ffffff02ff36ffff04ff02ffff04ff09ffff04ff820affffff04ff
    ff0bff2cff2d80ffff04ff15ff80808080808080ff8216ff80ffff04ff8205ff
    ffff04ff820bffff808080808080808080808080ff02ff2affff04ff02ffff04
    ff5fffff04ff3bffff04ffff02ffff03ff17ffff01ff09ff2dffff0bff27ffff
    02ff36ffff04ff02ffff04ff29ffff04ff57ffff04ffff0bff2cff81b980ffff
    04ff59ff80808080808080ff81b78080ff8080ff0180ffff04ff17ffff04ff05
    ffff04ff8202ffffff04ffff04ffff04ff24ffff04ffff0bff7cff2fff82017f
    80ff808080ffff04ffff04ff30ffff04ffff0bff81bfffff0bff7cff15ffff10
    ff82017fffff11ff8202dfff2b80ff8202ff808080ff808080ff138080ff8080
    8080808080808080ff018080
    "
);

const SETTLEMENT_PAYMENT_V1: [u8; 267] = hex!(
    "
    ff02ffff01ff02ff0affff04ff02ffff04ff03ff80808080ffff04ffff01ffff
    333effff02ffff03ff05ffff01ff04ffff04ff0cffff04ffff02ff1effff04ff
    02ffff04ff09ff80808080ff808080ffff02ff16ffff04ff02ffff04ff19ffff
    04ffff02ff0affff04ff02ffff04ff0dff80808080ff808080808080ff8080ff
    0180ffff02ffff03ff05ffff01ff04ffff04ff08ff0980ffff02ff16ffff04ff
    02ffff04ff0dffff04ff0bff808080808080ffff010b80ff0180ff02ffff03ff
    ff07ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff8080
    8080ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101
    ff058080ff0180ff018080
    "
);

static COMPRESSION_ZDICT: Lazy<Vec<u8>> = Lazy::new(|| {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE);
    bytes.extend_from_slice(&CAT_PUZZLE_V1);
    bytes.extend_from_slice(&SETTLEMENT_PAYMENT_V1);
    bytes.extend_from_slice(&SINGLETON_TOP_LAYER_V1_1);
    bytes.extend_from_slice(&NFT_STATE_LAYER);
    bytes.extend_from_slice(&NFT_OWNERSHIP_LAYER);
    bytes.extend_from_slice(&NFT_METADATA_UPDATER_DEFAULT);
    bytes.extend_from_slice(&NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES);
    bytes.extend_from_slice(&CAT_PUZZLE);
    bytes.extend_from_slice(&SETTLEMENT_PAYMENT);
    bytes
});

fn compress_offer_bytes(bytes: &[u8]) -> Result<Vec<u8>, DriverError> {
    let mut output = 6u16.to_be_bytes().to_vec();
    output.extend(zlib_compress(bytes, &COMPRESSION_ZDICT)?);
    Ok(output)
}

fn decompress_offer_bytes(bytes: &[u8]) -> Result<Vec<u8>, DriverError> {
    let version_bytes: [u8; 2] = bytes
        .get(0..2)
        .ok_or(DriverError::MissingVersionPrefix)?
        .try_into()?;

    let version = u16::from_be_bytes(version_bytes);

    if version > 6 {
        return Err(DriverError::UnsupportedVersion);
    }

    zlib_decompress(&bytes[2..], &COMPRESSION_ZDICT)
}

fn zlib_compress(input: &[u8], zdict: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut compress = Compress::new(Compression::new(6), true);
    compress.set_dictionary(zdict)?;
    let mut encoder = ZlibEncoder::new_with_compress(input, compress);
    let mut output = Vec::new();
    encoder.read_to_end(&mut output)?;
    Ok(output)
}

fn zlib_decompress(input: &[u8], zdict: &[u8]) -> Result<Vec<u8>, DriverError> {
    let mut decompress = Decompress::new(true);

    if decompress
        .decompress(input, &mut [], FlushDecompress::Finish)
        .is_ok()
    {
        return Err(DriverError::NotCompressed);
    }

    decompress.set_dictionary(zdict)?;
    let i = decompress.total_in();
    let mut decoder = ZlibDecoder::new_with_decompress(&input[usize::try_from(i)?..], decompress);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

fn encode_offer_data(offer: &[u8]) -> Result<String, DriverError> {
    let data = bech32::convert_bits(offer, 8, 5, true)?
        .into_iter()
        .map(u5::try_from_u8)
        .collect::<Result<Vec<_>, bech32::Error>>()?;
    Ok(bech32::encode("offer", data, Variant::Bech32m)?)
}

fn decode_offer_data(offer: &str) -> Result<Vec<u8>, DriverError> {
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
    use chia_protocol::SpendBundle;
    use chia_traits::Streamable;

    use super::*;

    const COMPRESSED_OFFER: &str = include_str!("./test_data/compressed.offer");
    const DECOMPRESSED_OFFER: &str = include_str!("./test_data/decompressed.offer");

    #[test]
    fn test_compression() {
        let decompressed_offer = hex::decode(DECOMPRESSED_OFFER.trim()).unwrap();
        let output = compress_offer_bytes(&decompressed_offer).unwrap();
        assert_eq!(hex::encode(output), COMPRESSED_OFFER.trim());
    }

    #[test]
    fn test_decompression() {
        let compressed_offer = hex::decode(COMPRESSED_OFFER.trim()).unwrap();
        let output = decompress_offer_bytes(&compressed_offer).unwrap();
        assert_eq!(hex::encode(output), DECOMPRESSED_OFFER.trim());
    }

    #[test]
    fn parse_spend_bundle() {
        let decompressed_offer = hex::decode(DECOMPRESSED_OFFER.trim()).unwrap();
        SpendBundle::from_bytes(&decompressed_offer).unwrap();
    }

    #[test]
    fn test_encode_decode_offer_data() {
        let offer = b"hello world";
        let encoded = encode_offer_data(offer).unwrap();
        let decoded = decode_offer_data(&encoded).unwrap();
        assert_eq!(offer, decoded.as_slice());
    }
}
