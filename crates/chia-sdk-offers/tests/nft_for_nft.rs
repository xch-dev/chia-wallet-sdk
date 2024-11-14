use chia_protocol::SpendBundle;
use chia_puzzles::{
    nft::NftMetadata,
    offer::{NotarizedPayment, Payment},
};
use chia_sdk_driver::{Launcher, Layer, NftMint, SpendContext, StandardLayer};
use chia_sdk_offers::{payment_assertion, Offer, OfferBuilder};
use chia_sdk_test::{sign_transaction, Simulator};
use chia_sdk_types::Conditions;

#[test]
fn test_nft_for_nft() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let (alice_secret_key, alice_pk, alice_puzzle_hash, alice_coin) = sim.child_p2(1, 1)?;
    let (bob_secret_key, bob_pk, bob_puzzle_hash, bob_coin) = sim.child_p2(1, 2)?;

    // Mint NFTs
    let (conditions, nft_alice) = Launcher::new(alice_coin.coin_id(), 1).mint_nft(
        &mut ctx,
        NftMint::new(NftMetadata::default(), alice_puzzle_hash, 0, None),
    )?;
    StandardLayer::new(alice_pk).spend(&mut ctx, alice_coin, conditions)?;

    let (conditions, nft_bob) = Launcher::new(bob_coin.coin_id(), 1).mint_nft(
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
    let nonce = Offer::nonce(vec![nft_alice.coin.coin_id()]);

    let puzzle_alice = nft_alice
        .info
        .clone()
        .into_layers(settlement)
        .construct_puzzle(&mut ctx)?;

    let puzzle_bob = nft_bob
        .info
        .clone()
        .into_layers(settlement)
        .construct_puzzle(&mut ctx)?;

    let (assertions, builder) = OfferBuilder::new(nonce)
        .request(
            &mut ctx,
            &puzzle_bob,
            vec![Payment::with_memos(
                alice_puzzle_hash,
                1,
                vec![alice_puzzle_hash.into()],
            )],
        )?
        .finish();

    let settlement_nft_alice = nft_alice.lock_settlement(
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
        ctx.tree_hash(puzzle_bob)
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

    let receive_nonce = Offer::nonce(vec![nft_bob.coin.coin_id()]);
    let receive_payment = NotarizedPayment {
        nonce: receive_nonce,
        payments: vec![Payment::with_memos(
            bob_puzzle_hash,
            1,
            vec![bob_puzzle_hash.into()],
        )],
    };

    let hash = ctx.tree_hash(puzzle_alice).into();
    let settlement_nft_bob = nft_bob.lock_settlement(
        &mut ctx,
        &StandardLayer::new(bob_pk),
        Vec::new(),
        Conditions::new().with(payment_assertion(hash, &receive_payment)),
    )?;

    let swapped_nft_alice = settlement_nft_bob.unlock_settlement(&mut ctx, payments)?;
    let swapped_nft_bob =
        settlement_nft_alice.unlock_settlement(&mut ctx, vec![receive_payment])?;

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, &[bob_secret_key])?;

    let spend_bundle = builder.bundle(SpendBundle::new(coin_spends, signature));
    sim.new_transaction(spend_bundle)?;

    assert_eq!(swapped_nft_alice.info.p2_puzzle_hash, alice_puzzle_hash);
    assert_eq!(swapped_nft_bob.info.p2_puzzle_hash, bob_puzzle_hash);

    Ok(())
}
