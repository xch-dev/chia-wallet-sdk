use chia_bls::Signature;
use chia_protocol::{Bytes32, CoinSpend};
use chia_puzzle_types::offer::NotarizedPayment;
use indexmap::IndexMap;

use crate::{OfferBuilder, Puzzle, Take};

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
