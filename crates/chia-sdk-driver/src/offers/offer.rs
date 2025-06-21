use chia_protocol::{Bytes32, SpendBundle};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::Allocator;
use indexmap::IndexMap;

use crate::{DriverError, ParsedOffer, Puzzle};

#[derive(Debug, Clone)]
pub struct Offer {
    spend_bundle: SpendBundle,
}

impl Offer {
    pub fn new(spend_bundle: SpendBundle) -> Self {
        Self { spend_bundle }
    }

    pub fn nonce(mut coin_ids: Vec<Bytes32>) -> Bytes32 {
        coin_ids.sort();
        coin_ids.tree_hash().into()
    }

    pub fn parse(self, allocator: &mut Allocator) -> Result<ParsedOffer, DriverError> {
        let mut parsed = ParsedOffer {
            aggregated_signature: self.spend_bundle.aggregated_signature,
            coin_spends: Vec::new(),
            requested_payments: IndexMap::new(),
        };

        for coin_spend in self.spend_bundle.coin_spends {
            if coin_spend.coin.parent_coin_info != Bytes32::default() {
                parsed.coin_spends.push(coin_spend);
                continue;
            }

            if coin_spend.coin.amount != 0 {
                parsed.coin_spends.push(coin_spend);
                continue;
            }

            let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
            let puzzle_hash = tree_hash(allocator, puzzle).into();

            if puzzle_hash != coin_spend.coin.puzzle_hash {
                return Err(DriverError::PuzzleMismatch);
            }

            let solution = coin_spend.solution.to_clvm(allocator)?;
            let settlement_solution = SettlementPaymentsSolution::from_clvm(allocator, solution)?;

            let puzzle = Puzzle::parse(allocator, puzzle);

            parsed
                .requested_payments
                .entry(puzzle_hash)
                .or_insert_with(|| (puzzle, Vec::new()))
                .1
                .extend(settlement_solution.notarized_payments);
        }

        Ok(parsed)
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
