use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzles::offer::{NotarizedPayment, Payment, SettlementPaymentsSolution};
use chia_sdk_driver::{DriverError, Puzzle, SpendContext};
use chia_sdk_types::AssertPuzzleAnnouncement;
use clvm_traits::ToClvm;
use clvmr::Allocator;
use indexmap::IndexMap;

use crate::{Offer, ParsedOffer};

#[derive(Debug, Clone)]
pub struct OfferBuilder<T> {
    data: T,
}

#[derive(Debug, Clone)]
pub struct Make {
    nonce: Bytes32,
    requested_payments: IndexMap<Bytes32, (Puzzle, Vec<NotarizedPayment>)>,
    announcements: Vec<AssertPuzzleAnnouncement>,
}

#[derive(Debug, Clone)]
pub struct Partial {
    requested_payments: IndexMap<Bytes32, (Puzzle, Vec<NotarizedPayment>)>,
}

#[derive(Debug, Clone)]
pub struct Take {
    parsed_offer: ParsedOffer,
}

impl OfferBuilder<Make> {
    pub fn new(nonce: Bytes32) -> Self {
        Self {
            data: Make {
                nonce,
                requested_payments: IndexMap::new(),
                announcements: Vec::new(),
            },
        }
    }

    /// Adds a list of requested payments for a given puzzle.
    /// It will use the nonce to create a new [`NotarizedPayment`] and add it to the requested payments.
    pub fn request<P>(
        mut self,
        ctx: &mut SpendContext,
        puzzle: &P,
        payments: Vec<Payment>,
    ) -> Result<Self, DriverError>
    where
        P: ToClvm<Allocator>,
    {
        let puzzle_ptr = ctx.alloc(puzzle)?;
        let puzzle_hash = ctx.tree_hash(puzzle_ptr).into();
        let puzzle = Puzzle::parse(&ctx.allocator, puzzle_ptr);

        let notarized_payment = NotarizedPayment {
            nonce: self.data.nonce,
            payments,
        };
        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

        self.data
            .requested_payments
            .entry(puzzle_hash)
            .or_insert_with(|| (puzzle, Vec::new()))
            .1
            .push(notarized_payment);

        self.data.announcements.push(AssertPuzzleAnnouncement::new(
            puzzle_hash,
            notarized_payment_hash,
        ));

        Ok(self)
    }

    /// This will create a new [`OfferBuilder`] with the requested payments frozen.
    /// It returns a list of announcements that can be asserted by the maker side.
    pub fn finish(self) -> (Vec<AssertPuzzleAnnouncement>, OfferBuilder<Partial>) {
        let partial = OfferBuilder {
            data: Partial {
                requested_payments: self.data.requested_payments,
            },
        };
        (self.data.announcements, partial)
    }
}

impl OfferBuilder<Partial> {
    pub fn bundle(
        self,
        ctx: &mut SpendContext,
        partial_spend_bundle: SpendBundle,
    ) -> Result<Offer, DriverError> {
        let mut spend_bundle = partial_spend_bundle;

        for (puzzle_hash, (puzzle, notarized_payments)) in self.data.requested_payments {
            let puzzle_reveal = ctx.serialize(&puzzle.ptr())?;
            let solution = ctx.serialize(&SettlementPaymentsSolution { notarized_payments })?;

            spend_bundle.coin_spends.push(CoinSpend {
                coin: Coin::new(Bytes32::default(), puzzle_hash, 0),
                puzzle_reveal,
                solution,
            });
        }

        Ok(spend_bundle.into())
    }

    /// This will use the partial spend bundle to create a new [`OfferBuilder`] for taking.
    pub fn take(self, partial_spend_bundle: SpendBundle) -> OfferBuilder<Take> {
        OfferBuilder {
            data: Take {
                parsed_offer: ParsedOffer {
                    coin_spends: partial_spend_bundle.coin_spends,
                    aggregated_signature: partial_spend_bundle.aggregated_signature,
                    requested_payments: self.data.requested_payments,
                },
            },
        }
    }
}

impl OfferBuilder<Take> {
    pub fn from_parsed_offer(parsed_offer: ParsedOffer) -> Self {
        Self {
            data: Take { parsed_offer },
        }
    }

    pub fn fulfill(&mut self) -> Option<(Puzzle, Vec<NotarizedPayment>)> {
        Some(
            self.data
                .parsed_offer
                .requested_payments
                .shift_remove_index(0)?
                .1,
        )
    }

    pub fn bundle(self, other_spend_bundle: SpendBundle) -> SpendBundle {
        SpendBundle::aggregate(&[
            SpendBundle::new(
                self.data.parsed_offer.coin_spends,
                self.data.parsed_offer.aggregated_signature,
            ),
            other_spend_bundle,
        ])
    }
}
