use chia_protocol::SpendBundle;
use chia_puzzles::{
    nft::NftMetadata,
    offer::{NotarizedPayment, Payment},
};
use chia_sdk_driver::{Launcher, Layer, NftMint, SpendContext, StandardLayer};
use chia_sdk_offers::{payment_assertion, Offer, OfferBuilder};
use chia_sdk_test::{sign_transaction, Simulator};
use chia_sdk_types::Conditions;

mod common;

#[test]
fn test_nft_for_nft() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let (alice_secret_key, alice_pk, alice_puzzle_hash, alice_coin) = sim.child_p2(1, 1)?;
    let (bob_secret_key, bob_pk, bob_puzzle_hash, bob_coin) = sim.child_p2(1, 2)?;

    // Mint NFTs
    let (conditions, nft_a) = Launcher::new(alice_coin.coin_id(), 1).mint_nft(
        &mut ctx,
        NftMint::new(NftMetadata::default(), alice_puzzle_hash, 0, None),
    )?;
    StandardLayer::new(alice_pk).spend(&mut ctx, alice_coin, conditions)?;

    let (conditions, nft_b) = Launcher::new(bob_coin.coin_id(), 1).mint_nft(
        &mut ctx,
        NftMint::new(NftMetadata::default(), bob_puzzle_hash, 0, None),
    )?;
    StandardLayer::new(bob_pk).spend(&mut ctx, bob_coin, conditions)?;

    let coin_spends = ctx.take();
    sim.spend_coins(
        coin_spends,
        &[alice_secret_key.clone(), bob_secret_key.clone()],
    )?;

    // Create offer
    let settlement = ctx.settlement_payments_puzzle()?;
    let nonce = Offer::nonce(vec![nft_a.coin.coin_id()]);

    let puzzle_b = nft_b
        .info
        .clone()
        .into_layers(settlement)
        .construct_puzzle(&mut ctx)?;

    let puzzle_a = nft_a
        .info
        .clone()
        .into_layers(settlement)
        .construct_puzzle(&mut ctx)?;

    let puzzle_a_hash = ctx.tree_hash(puzzle_a);

    let (assertions, builder) = OfferBuilder::new(nonce)
        .request(
            &mut ctx,
            &puzzle_b,
            vec![Payment::with_memos(
                alice_puzzle_hash,
                1,
                vec![alice_puzzle_hash.into()],
            )],
        )?
        .finish();

    let settlement_nft_a = nft_a.lock_settlement(
        &mut ctx,
        &StandardLayer::new(alice_pk),
        Vec::new(),
        Conditions::new().extend(assertions),
    )?;

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, &[alice_secret_key])?;

    // Fulfill offer
    let mut builder = builder.take(SpendBundle::new(coin_spends, signature));

    let (fulfill_puzzle, payments) = builder.fulfill().expect("cannot fulfill offer");
    assert_eq!(
        fulfill_puzzle.curried_puzzle_hash(),
        ctx.tree_hash(puzzle_b)
    );
    assert_eq!(
        payments,
        [NotarizedPayment {
            nonce,
            payments: vec![Payment::with_memos(
                alice_puzzle_hash,
                1,
                vec![alice_puzzle_hash.into()],
            )],
        }]
    );

    let receive_nonce = Offer::nonce(vec![nft_b.coin.coin_id()]);
    let receive_payment = NotarizedPayment {
        nonce: receive_nonce,
        payments: vec![Payment::with_memos(
            bob_puzzle_hash,
            1,
            vec![bob_puzzle_hash.into()],
        )],
    };

    let settlement_nft_b = nft_b.lock_settlement(
        &mut ctx,
        &StandardLayer::new(bob_pk),
        Vec::new(),
        Conditions::new().with(payment_assertion(puzzle_a_hash.into(), &receive_payment)),
    )?;

    let new_nft_b = settlement_nft_b.unlock_settlement(&mut ctx, payments)?;
    let new_nft_a = settlement_nft_a.unlock_settlement(&mut ctx, vec![receive_payment])?;

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, &[bob_secret_key])?;

    let spend_bundle = builder.bundle(SpendBundle::new(coin_spends, signature));
    sim.new_transaction(spend_bundle)?;

    assert_eq!(new_nft_a.info.p2_puzzle_hash, bob_puzzle_hash);
    assert_eq!(new_nft_b.info.p2_puzzle_hash, alice_puzzle_hash);

    Ok(())
}
