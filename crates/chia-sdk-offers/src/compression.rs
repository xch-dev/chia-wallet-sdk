use std::io::Read;

use chia_protocol::SpendBundle;
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
use chia_traits::Streamable;
use flate2::{
    read::{ZlibDecoder, ZlibEncoder},
    Compress, Compression, Decompress, FlushDecompress,
};

use crate::CompressionError;

const MIN_VERSION: u16 = 6;
const MAX_VERSION: u16 = 6;

macro_rules! define_compression_versions {
    ( $( $version:expr => $( $bytes:expr ),+ ; )+ ) => {
        fn zdict_for_version(version: u16) -> Vec<u8> {
            let mut bytes = Vec::new();
            $( if version >= $version {
                $( bytes.extend_from_slice(&$bytes); )+
            } )+
            bytes
        }

        /// Returns the required compression version for the given puzzle reveals.

        pub fn required_compression_version(puzzles: Vec<Vec<u8>>) -> u16 {
            let mut required_version = MIN_VERSION;
            $( {
                $( if required_version < $version && puzzles.iter().any(|puzzle| puzzle == &$bytes) {
                    required_version = $version;
                } )+
            } )+
            required_version
        }
    };
}

define_compression_versions!(
    1 => STANDARD_PUZZLE, CAT_PUZZLE_V1;
    2 => SETTLEMENT_PAYMENTS_PUZZLE_V1;
    3 => SINGLETON_TOP_LAYER_PUZZLE, NFT_STATE_LAYER_PUZZLE,
         NFT_OWNERSHIP_LAYER_PUZZLE, NFT_METADATA_UPDATER_PUZZLE,
         NFT_ROYALTY_TRANSFER_PUZZLE;
    4 => CAT_PUZZLE;
    5 => SETTLEMENT_PAYMENTS_PUZZLE;
    6 => [0; 0]; // Purposefully break backwards compatibility.
);

/// Compresses an offer spend bundle.
pub fn compress_offer(spend_bundle: SpendBundle) -> Result<Vec<u8>, CompressionError> {
    let bytes = spend_bundle.to_bytes()?;
    let version = required_compression_version(
        spend_bundle
            .coin_spends
            .into_iter()
            .map(|cs| cs.puzzle_reveal.to_vec())
            .collect(),
    );
    compress_offer_bytes(&bytes, version)
}

/// Compresses an offer spend bundle from bytes.
pub fn compress_offer_bytes(bytes: &[u8], version: u16) -> Result<Vec<u8>, CompressionError> {
    let mut output = version.to_be_bytes().to_vec();
    let zdict = zdict_for_version(version);
    output.extend(compress(bytes, &zdict)?);
    Ok(output)
}

fn compress(input: &[u8], zdict: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut compress = Compress::new(Compression::new(6), true);
    compress.set_dictionary(zdict)?;
    let mut encoder = ZlibEncoder::new_with_compress(input, compress);
    let mut output = Vec::new();
    encoder.read_to_end(&mut output)?;
    Ok(output)
}

/// Decompresses an offer spend bundle.
pub fn decompress_offer(bytes: &[u8]) -> Result<SpendBundle, CompressionError> {
    let decompressed_bytes = decompress_offer_bytes(bytes)?;
    Ok(SpendBundle::from_bytes(&decompressed_bytes)?)
}

/// Decompresses an offer spend bundle into bytes.
pub fn decompress_offer_bytes(bytes: &[u8]) -> Result<Vec<u8>, CompressionError> {
    let version_bytes: [u8; 2] = bytes
        .get(0..2)
        .ok_or(CompressionError::MissingVersionPrefix)?
        .try_into()?;

    let version = u16::from_be_bytes(version_bytes);

    if version > MAX_VERSION {
        return Err(CompressionError::UnsupportedVersion);
    }

    let zdict = zdict_for_version(version);

    decompress(&bytes[2..], &zdict)
}

fn decompress(input: &[u8], zdict: &[u8]) -> Result<Vec<u8>, CompressionError> {
    let mut decompress = Decompress::new(true);

    if decompress
        .decompress(input, &mut [], FlushDecompress::Finish)
        .is_ok()
    {
        return Err(CompressionError::NotCompressed);
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
        for version in MIN_VERSION..=MAX_VERSION {
            let output = compress_offer_bytes(&decompressed_offer, version).unwrap();
            assert_eq!(hex::encode(output), COMPRESSED_OFFER.trim());
        }
    }

    #[test]
    fn test_decompression() {
        let compressed_offer = hex::decode(COMPRESSED_OFFER.trim()).unwrap();
        for _ in MIN_VERSION..=MAX_VERSION {
            let output = decompress_offer_bytes(&compressed_offer).unwrap();
            assert_eq!(hex::encode(output), DECOMPRESSED_OFFER.trim());
        }
    }

    #[test]
    fn parse_spend_bundle() {
        let decompressed_offer = hex::decode(DECOMPRESSED_OFFER.trim()).unwrap();
        SpendBundle::from_bytes(&decompressed_offer).unwrap();
    }

    const COMPRESSED_OFFER: &str = include_str!("../test_data/compressed.offer");
    const DECOMPRESSED_OFFER: &str = include_str!("../test_data/decompressed.offer");
}
