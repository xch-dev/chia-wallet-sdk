use std::collections::HashMap;

use anyhow::bail;
use chia_bls::{DerivableKey, PublicKey};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzles::{
    nft::NftMetadata,
    offer::{
        NotarizedPayment, Payment, SettlementPaymentsSolution, SETTLEMENT_PAYMENTS_PUZZLE_HASH,
    },
    standard::StandardArgs,
};
use chia_sdk_driver::{Cat, Layer, Nft, Puzzle, SettlementLayer, SpendContext};
use chia_sdk_test::{secret_key, Simulator};
use chia_sdk_types::AssertPuzzleAnnouncement;

use crate::Offer;

struct CoinData {
    /// Public keys by puzzle hash.
    p2: HashMap<Bytes32, PublicKey>,
    /// CATs by asset id.
    cats: HashMap<Bytes32, Cat>,
    /// NFTs by launcher id.
    nfts: HashMap<Bytes32, Nft<NftMetadata>>,
}

fn take_offer(
    coins: &CoinData,
    offer: Offer,
    target_puzzle_hash: Bytes32,
) -> anyhow::Result<SpendBundle> {
    let mut ctx = SpendContext::new();

    let parsed = offer.parse(&mut ctx.allocator)?;

    let nonce = Bytes32::default();
    let mut announcements = Vec::new();

    for coin_spend in &parsed.coin_spends {
        let puzzle_ptr = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(&ctx.allocator, puzzle_ptr);

        let is_p2 = coins.p2.contains_key(&puzzle.curried_puzzle_hash().into());

        let notarized_payment = NotarizedPayment {
            nonce,
            payments: vec![Payment::with_memos(
                target_puzzle_hash,
                coin_spend.coin.amount,
                if is_p2 {
                    Vec::new()
                } else {
                    vec![target_puzzle_hash.into()]
                },
            )],
        };

        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

        let inner_solution = SettlementPaymentsSolution {
            notarized_payments: vec![notarized_payment],
        };

        let puzzle_hash = if is_p2 {
            let coin = Coin::new(
                coin_spend.coin.coin_id(),
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                coin_spend.coin.amount,
            );

            let coin_spend =
                SettlementLayer.construct_coin_spend(&mut ctx, coin, inner_solution)?;
            ctx.insert(coin_spend);

            coin.puzzle_hash
        } else {
            bail!("Unsupported offered puzzle type");
        };

        announcements.push(AssertPuzzleAnnouncement::new(
            puzzle_hash,
            notarized_payment_hash,
        ));
    }

    let mut builder = parsed.take();

    while let Some((puzzle, notarized_payments)) = builder.fulfill() {
        if puzzle.curried_puzzle_hash() != SETTLEMENT_PAYMENTS_PUZZLE_HASH {
            bail!("Unsupported requested puzzle type");
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

    Ok()
}

#[tokio::test]
async fn test_p2_offer() -> anyhow::Result<()> {
    let sim = Simulator::new().await?;
    let root_sk = secret_key()?;

    let bob = root_sk.derive_unhardened(0);
    let alice = root_sk.derive_unhardened(1);

    let bob_coin = sim.mint_coin(StandardArgs::curry_tree_hash(bob.public_key()).into(), 1000);
    let alice_coin = sim.mint_coin(
        StandardArgs::curry_tree_hash(alice.public_key()).into(),
        3000,
    );

    Ok(())
}
