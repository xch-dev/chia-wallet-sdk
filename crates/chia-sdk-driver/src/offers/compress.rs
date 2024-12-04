use std::io::Read;

use chia_puzzles::{
    cat::{CAT_PUZZLE, CAT_PUZZLE_V1},
    nft::{
        NFT_METADATA_UPDATER_PUZZLE, NFT_OWNERSHIP_LAYER_PUZZLE, NFT_ROYALTY_TRANSFER_PUZZLE,
        NFT_STATE_LAYER_PUZZLE,
    },
    offer::{SETTLEMENT_PAYMENTS_PUZZLE, SETTLEMENT_PAYMENTS_PUZZLE_V1},
    singleton::SINGLETON_TOP_LAYER_PUZZLE,
    standard::STANDARD_PUZZLE,
};
use flate2::{
    read::{ZlibDecoder, ZlibEncoder},
    Compress, Compression, Decompress, FlushDecompress,
};
use once_cell::sync::Lazy;

use crate::OfferError;

static COMPRESSION_ZDICT: Lazy<Vec<u8>> = Lazy::new(|| {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&STANDARD_PUZZLE);
    bytes.extend_from_slice(&CAT_PUZZLE_V1);
    bytes.extend_from_slice(&SETTLEMENT_PAYMENTS_PUZZLE_V1);
    bytes.extend_from_slice(&SINGLETON_TOP_LAYER_PUZZLE);
    bytes.extend_from_slice(&NFT_STATE_LAYER_PUZZLE);
    bytes.extend_from_slice(&NFT_OWNERSHIP_LAYER_PUZZLE);
    bytes.extend_from_slice(&NFT_METADATA_UPDATER_PUZZLE);
    bytes.extend_from_slice(&NFT_ROYALTY_TRANSFER_PUZZLE);
    bytes.extend_from_slice(&CAT_PUZZLE);
    bytes.extend_from_slice(&SETTLEMENT_PAYMENTS_PUZZLE);
    bytes
});

pub fn compress_offer_bytes(bytes: &[u8]) -> Result<Vec<u8>, OfferError> {
    let mut output = 6u16.to_be_bytes().to_vec();
    output.extend(zlib_compress(bytes, &COMPRESSION_ZDICT)?);
    Ok(output)
}

pub fn decompress_offer_bytes(bytes: &[u8]) -> Result<Vec<u8>, OfferError> {
    let version_bytes: [u8; 2] = bytes
        .get(0..2)
        .ok_or(OfferError::MissingVersionPrefix)?
        .try_into()?;

    let version = u16::from_be_bytes(version_bytes);

    if version > 6 {
        return Err(OfferError::UnsupportedVersion);
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

fn zlib_decompress(input: &[u8], zdict: &[u8]) -> Result<Vec<u8>, OfferError> {
    let mut decompress = Decompress::new(true);

    if decompress
        .decompress(input, &mut [], FlushDecompress::Finish)
        .is_ok()
    {
        return Err(OfferError::NotCompressed);
    }

    decompress.set_dictionary(zdict)?;
    let i = decompress.total_in();
    let mut decoder = ZlibDecoder::new_with_decompress(&input[usize::try_from(i)?..], decompress);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use chia_protocol::SpendBundle;
    use chia_traits::Streamable;

    use super::*;

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

    const COMPRESSED_OFFER: &str = include_str!("./test_data/compressed.offer");
    const DECOMPRESSED_OFFER: &str = include_str!("./test_data/decompressed.offer");
}
