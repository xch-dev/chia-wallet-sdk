use super::*;

#[test]
fn test_aggregate_sells_same_price_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 1000)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(1500));

    let coin = t.fund(&carol, 21_500);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_different_prices_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 1000)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(30_000))?;
    let offer = t.aggregate(&[alice_offer.clone(), bob_offer.clone()])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 3000));

    // Extending parsed offers keeps each maker's trade prices too
    let mut extended = t.aggregate(&[alice_offer])?;
    extended.extend(t.aggregate(&[bob_offer])?)?;
    assert_eq!(extended.requested_royalty_amounts()?, xch(500 + 3000));

    let coin = t.fund(&carol, 43_500);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_different_prices_same_royalty() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(30_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 1500));

    let coin = t.fund(&carol, 42_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_identical_royalties_for_different_nfts() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Both NFTs pay the same royalty to the same address, but each asserts its own payment
    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(500 + 500));

    let coin = t.fund(&carol, 21_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&alice_nft, carol.puzzle_hash));
    assert!(t.owns_nft(&bob_nft, carol.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_multi_nft_offer_with_single_nft_offer() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    // Alice's two NFTs each commit to half of her price
    let first_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&alice, ROYALTY_B, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_C, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[first_nft, second_nft], &xch(30_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(750 + 750 + 500));

    let coin = t.fund(&carol, 42_000);
    t.take(&carol, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 750);
    assert_eq!(t.balance(ROYALTY_B), 750);
    assert_eq!(t.balance(ROYALTY_C), 500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_sells_for_different_assets() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let carol_cat = t.issue_cat(&carol, 10_500)?;
    let asset_id = carol_cat.info.asset_id;
    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[alice_nft], &xch(10_000))?;
    let bob_offer = t.sell_nfts(&bob, &[bob_nft], &cat(asset_id, 10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: 500,
            cats: indexmap! { asset_id => 500 },
        }
    );

    let coin = t.fund(&carol, 10_500);
    t.take(&carol, &offer, |spends| {
        spends.add(coin);
        spends.add(carol_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_A), 0);
    assert_eq!(t.balance(ROYALTY_B), 0);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_B), 500);
    assert_eq!(t.balance(carol.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&carol_cat, carol.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_aggregate_nft_sale_with_unrelated_cat_trade() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let carol_cat = t.issue_cat(&carol, 21_000)?;
    let asset_id = carol_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(10_000))?;
    let bob_offer = t.trade_xch_for_cat(&bob, 5000, asset_id, 20_000)?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    // Only Alice's NFT is sold, and only for XCH
    assert_eq!(offer.requested_royalty_amounts()?, xch(500));

    let coin = t.fund(&carol, 5500);
    t.take(&carol, &offer, |spends| {
        spends.add(coin);
        spends.add(carol_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.cat_balance(&carol_cat, ROYALTY_A), 0);
    assert_eq!(t.balance(carol.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&carol_cat, carol.puzzle_hash), 1000);

    Ok(())
}
