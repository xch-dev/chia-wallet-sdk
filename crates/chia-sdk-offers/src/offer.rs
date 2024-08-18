use chia_protocol::{Bytes32, SpendBundle};
use chia_traits::Streamable;
use clvm_utils::ToTreeHash;

use crate::{
    compress_offer_bytes, decode_offer_data, decompress_offer_bytes, encode_offer_data, Make,
    OfferBuilder, OfferError, Take,
};

#[derive(Debug, Clone)]
pub struct Offer {
    spend_bundle: SpendBundle,
}

impl Offer {
    pub fn new(spend_bundle: SpendBundle) -> Self {
        Self { spend_bundle }
    }

    pub fn build(coin_ids: Vec<Bytes32>) -> OfferBuilder<Make> {
        Self::build_with_nonce(Self::nonce(coin_ids))
    }

    pub fn build_with_nonce(nonce: Bytes32) -> OfferBuilder<Make> {
        OfferBuilder::new(nonce)
    }

    pub fn take(self) -> OfferBuilder<Take> {
        // OfferBuilder::from(self)
        todo!()
    }

    pub fn nonce(mut coin_ids: Vec<Bytes32>) -> Bytes32 {
        coin_ids.sort();
        coin_ids.tree_hash().into()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, OfferError> {
        Ok(self.spend_bundle.to_bytes()?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, OfferError> {
        Ok(SpendBundle::from_bytes(bytes)?.into())
    }

    pub fn compress(&self) -> Result<Vec<u8>, OfferError> {
        compress_offer_bytes(&self.to_bytes()?)
    }

    pub fn decompress(bytes: &[u8]) -> Result<Self, OfferError> {
        Self::from_bytes(&decompress_offer_bytes(bytes)?)
    }

    pub fn encode(&self) -> Result<String, OfferError> {
        encode_offer_data(&self.compress()?)
    }

    pub fn decode(text: &str) -> Result<Self, OfferError> {
        Self::decompress(&decode_offer_data(text)?)
    }
}

impl From<SpendBundle> for Offer {
    fn from(spend_bundle: SpendBundle) -> Self {
        Self::new(spend_bundle)
    }
}

impl From<Offer> for SpendBundle {
    fn from(offer: Offer) -> Self {
        offer.spend_bundle
    }
}
