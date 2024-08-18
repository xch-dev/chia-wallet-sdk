use chia_protocol::{Bytes32, CoinSpend, SpendBundle};
use chia_puzzles::offer::{NotarizedPayment, Payment};
use chia_sdk_driver::{DriverError, Puzzle, SpendContext};
use chia_sdk_types::AssertPuzzleAnnouncement;
use clvm_traits::ToClvm;
use clvmr::Allocator;
use indexmap::IndexMap;

use crate::Offer;

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
    partial_spend_bundle: SpendBundle,
    requested_payments: IndexMap<Bytes32, (Puzzle, Vec<NotarizedPayment>)>,
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
    pub fn bundle(self) -> Offer {
        // Offer::new(spend_bundle)
        todo!()
    }

    /// This will use the partial spend bundle to create a new [`OfferBuilder`] for taking.
    pub fn take(self, partial_spend_bundle: SpendBundle) -> OfferBuilder<Take> {
        OfferBuilder {
            data: Take {
                partial_spend_bundle,
                requested_payments: self.data.requested_payments,
            },
        }
    }
}

impl OfferBuilder<Take> {
    pub fn from(offer: Offer) -> Self {
        todo!()
    }

    pub fn coin_spends(&self) -> &[CoinSpend] {
        &self.data.partial_spend_bundle.coin_spends
    }

    pub fn fulfill(&mut self) -> Option<(Puzzle, Vec<NotarizedPayment>)> {
        Some(self.data.requested_payments.shift_remove_index(0)?.1)
    }

    pub fn bundle(self, other_spend_bundle: SpendBundle) -> SpendBundle {
        SpendBundle::aggregate(&[self.data.partial_spend_bundle, other_spend_bundle])
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::DerivableKey;
    use chia_protocol::Coin;
    use chia_puzzles::{
        offer::{SettlementPaymentsSolution, SETTLEMENT_PAYMENTS_PUZZLE_HASH},
        standard::StandardArgs,
    };
    use chia_sdk_driver::{Layer, SettlementLayer};
    use chia_sdk_test::{secret_key, sign_transaction, Simulator};
    use chia_sdk_types::Conditions;

    use super::*;

    struct Sides<Bob, Alice> {
        bob: Bob,
        alice: Alice,
    }

    #[tokio::test]
    async fn test_p2_offer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let root_sk = secret_key()?;

        // Let 0 be Bob, and 1 be Alice.
        let sk = Sides {
            bob: root_sk.derive_unhardened(0),
            alice: root_sk.derive_unhardened(1),
        };
        let pk = Sides {
            bob: sk.bob.public_key(),
            alice: sk.alice.public_key(),
        };
        let puzzle_hash = Sides {
            bob: StandardArgs::curry_tree_hash(pk.bob).into(),
            alice: StandardArgs::curry_tree_hash(pk.alice).into(),
        };
        let coin = Sides {
            bob: sim.mint_coin(puzzle_hash.bob, 1000).await,
            alice: sim.mint_coin(puzzle_hash.alice, 3000).await,
        };

        let settlement_puzzle = ctx.settlement_payments_puzzle()?;

        // We use the coins being offered to calculate the nonce.
        // Request a payment from Alice's coin amount to Bob's puzzle hash.
        let (announcements, partial) = Offer::build(vec![coin.bob.coin_id()])
            .request(
                ctx,
                &settlement_puzzle,
                vec![Payment::new(puzzle_hash.bob, coin.alice.amount)],
            )?
            .finish();

        // Send Bob's coin to the settlement puzzle in order to offer it.
        ctx.spend_p2_coin(
            coin.bob,
            pk.bob,
            Conditions::new().extend(announcements).create_coin(
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                coin.bob.amount,
                Vec::new(),
            ),
        )?;

        // Create the partial spend bundle for Bob's side and start taking the offer for Alice.
        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[sk.bob], &sim.config().constants)?;

        let mut take = partial.take(SpendBundle::new(coin_spends, signature));

        // Determine the announcements required to securely take the offer.
        let mut announcements = Vec::new();

        for coin_spend in take.coin_spends() {
            let notarized_payment = NotarizedPayment {
                nonce: Offer::nonce(vec![coin.alice.coin_id()]),
                payments: vec![Payment::new(puzzle_hash.alice, coin_spend.coin.amount)],
            };
            let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
            let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);
            announcements.push(AssertPuzzleAnnouncement::new(
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                notarized_payment_hash,
            ));

            // Receive the offered coin.
            let coin_spend = SettlementLayer.construct_coin_spend(
                ctx,
                Coin::new(
                    coin_spend.coin.coin_id(),
                    SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                    coin_spend.coin.amount,
                ),
                SettlementPaymentsSolution {
                    notarized_payments: vec![notarized_payment],
                },
            )?;
            ctx.insert(coin_spend);
        }

        // Fulfill the offer by taking the requested payment.
        while let Some((puzzle, notarized_payments)) = take.fulfill() {
            if puzzle.curried_puzzle_hash() != SETTLEMENT_PAYMENTS_PUZZLE_HASH {
                unreachable!("Cannot fulfill the offer");
            }

            // Spend Alice's coin to the settlement puzzle in order to take the offer.
            ctx.spend_p2_coin(
                coin.alice,
                pk.alice,
                Conditions::new().extend(announcements.clone()).create_coin(
                    SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                    coin.alice.amount,
                    Vec::new(),
                ),
            )?;

            // Spend the settlement coin with the requested payments.
            let coin_spend = SettlementLayer.construct_coin_spend(
                ctx,
                Coin::new(
                    coin.alice.coin_id(),
                    SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                    coin.alice.amount,
                ),
                SettlementPaymentsSolution { notarized_payments },
            )?;
            ctx.insert(coin_spend);
        }

        // Alice can now aggregate the spend bundles to finalize the offer.
        let coin_spends = ctx.take();
        let signature = sign_transaction(&coin_spends, &[sk.alice], &sim.config().constants)?;
        let spend_bundle = take.bundle(SpendBundle::new(coin_spends, signature));

        // The offer is now finalized and can be sent to the blockchain.
        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }
}
