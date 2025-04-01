use chia_protocol::{Coin, SpendBundle};
use chia_puzzle_types::{
    nft::NftMetadata,
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_test::{sign_transaction, Simulator};
use chia_sdk_types::{conditions::TradePrice, puzzles::SettlementPayment, Conditions};

use crate::{
    calculate_nft_royalty, calculate_nft_trace_price, payment_assertion, Launcher, Layer, NftMint,
    Offer, OfferBuilder, SettlementLayer, SpendContext, StandardLayer,
};

#[test]
fn test_nft_for_xch() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let alice = sim.bls(1);
    let bob = sim.bls(1_000_000_000_000);

    // Mint NFT on maker side
    let (conditions, nft) = Launcher::new(alice.coin.coin_id(), 1).mint_nft(
        &mut ctx,
        NftMint::new(NftMetadata::default(), alice.puzzle_hash, 300, None),
    )?;
    let launcher_id = nft.info.launcher_id;
    StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, conditions)?;

    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends, &[alice.sk.clone()])?;

    // Create offer
    let settlement = ctx.alloc_mod::<SettlementPayment>()?;
    let nonce = Offer::nonce(vec![nft.coin.coin_id()]);

    let nft_puzzle = nft
        .info
        .clone()
        .into_layers(settlement)
        .construct_puzzle(&mut ctx)?;

    let nft_trade_price =
        calculate_nft_trace_price(500_000_000_000, 1).expect("failed to calculate trade price");

    let nft_royalty = calculate_nft_royalty(nft_trade_price, nft.info.royalty_ten_thousandths)
        .expect("failed to calculate royalty");

    let (assertions, builder) = OfferBuilder::new(nonce)
        .request(
            &mut ctx,
            &settlement,
            vec![Payment::new(alice.puzzle_hash, 500_000_000_000)],
        )?
        .request_with_nonce(
            &mut ctx,
            &settlement,
            launcher_id,
            vec![Payment::with_memos(
                alice.puzzle_hash,
                nft_royalty,
                vec![alice.puzzle_hash.into()],
            )],
        )?
        .finish();

    let settlement_nft = nft.lock_settlement(
        &mut ctx,
        &StandardLayer::new(alice.pk),
        vec![TradePrice {
            amount: nft_trade_price,
            puzzle_hash: SETTLEMENT_PAYMENT_HASH.into(),
        }],
        Conditions::new().extend(assertions),
    )?;

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, &[alice.sk.clone()])?;

    // Fulfill offer
    let mut builder = builder.take(SpendBundle::new(coin_spends, signature));

    let (fulfill_puzzle, payments) = builder.fulfill().expect("cannot fulfill offer");
    assert_eq!(
        fulfill_puzzle.curried_puzzle_hash(),
        SETTLEMENT_PAYMENT_HASH.into()
    );
    assert_eq!(
        payments,
        [
            NotarizedPayment {
                nonce,
                payments: vec![Payment::new(alice.puzzle_hash, 500_000_000_000)],
            },
            NotarizedPayment {
                nonce: launcher_id,
                payments: vec![Payment::with_memos(
                    alice.puzzle_hash,
                    nft_royalty,
                    vec![alice.puzzle_hash.into()],
                )],
            }
        ]
    );

    let receive_nonce = Offer::nonce(vec![bob.coin.coin_id()]);
    let receive_payment = NotarizedPayment {
        nonce: receive_nonce,
        payments: vec![Payment::with_memos(
            bob.puzzle_hash,
            1,
            vec![bob.puzzle_hash.into()],
        )],
    };

    let total_amount = 500_000_000_000 + nft_royalty;

    let hash = ctx.tree_hash(nft_puzzle).into();

    StandardLayer::new(bob.pk).spend(
        &mut ctx,
        bob.coin,
        Conditions::new()
            .create_coin(SETTLEMENT_PAYMENT_HASH.into(), total_amount, None)
            .with(payment_assertion(hash, &receive_payment)),
    )?;

    let settlement_coin = Coin::new(
        bob.coin.coin_id(),
        SETTLEMENT_PAYMENT_HASH.into(),
        total_amount,
    );

    let coin_spend = SettlementLayer.construct_coin_spend(
        &mut ctx,
        settlement_coin,
        SettlementPaymentsSolution {
            notarized_payments: payments,
        },
    )?;
    ctx.insert(coin_spend);

    let swapped_nft = settlement_nft.unlock_settlement(&mut ctx, vec![receive_payment])?;

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, &[bob.sk])?;

    let spend_bundle = builder.bundle(SpendBundle::new(coin_spends, signature));

    sim.new_transaction(spend_bundle)?;

    assert_eq!(swapped_nft.info.p2_puzzle_hash, bob.puzzle_hash);

    Ok(())
}
