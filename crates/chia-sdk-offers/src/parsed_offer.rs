use chia_bls::Signature;
use chia_protocol::{Bytes32, CoinSpend};
use chia_puzzles::offer::NotarizedPayment;
use chia_sdk_driver::Puzzle;
use indexmap::IndexMap;

use crate::{OfferBuilder, Take};

#[derive(Debug, Default, Clone)]
pub struct ParsedOffer {
    pub coin_spends: Vec<CoinSpend>,
    pub aggregated_signature: Signature,
    pub requested_payments: IndexMap<Bytes32, (Puzzle, Vec<NotarizedPayment>)>,
}

impl ParsedOffer {
    pub fn take(self) -> OfferBuilder<Take> {
        OfferBuilder::from_parsed_offer(self)
    }
}
