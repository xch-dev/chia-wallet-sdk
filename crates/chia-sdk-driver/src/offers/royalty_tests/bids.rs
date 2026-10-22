use super::*;

#[test]
fn test_bid_for_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::Xch(1999))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(59));
    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1967)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1999);
    assert_eq!(t.balance(ROYALTY_A), 59);

    Ok(())
}

#[test]
fn test_bid_for_nft_with_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::Cat(1000))?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(offer.offered_royalty_amounts()?, cat(asset_id, 30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![cat_trade_price(asset_id, 1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_bid_for_nft_with_revocable_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid(&alice, &nft, Price::RevocableCat(1000))?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(offer.offered_royalty_amounts()?, cat(asset_id, 30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![revocable_cat_trade_price(asset_id, 1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 30);

    Ok(())
}

#[test]
fn test_bid_for_nft_in_two_assets() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let offer = t.bid_in_assets(&alice, &nft, &[Price::Xch(1000), Price::Cat(2000)])?;
    let offer = t.aggregate(&[offer])?;
    let (&asset_id, cats) = offer.offered_coins().cats.first().expect("offered cat");
    let alice_cat = cats[0];

    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 30,
            cats: indexmap! { asset_id => 60 },
        }
    );
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1000), cat_trade_price(asset_id, 2000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(bob.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&alice_cat, bob.puzzle_hash), 2000);
    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&alice_cat, ROYALTY_A), 60);

    Ok(())
}

#[test]
fn test_aggregate_bids_same_price_different_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(10_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 1000));

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert!(t.owns_nft(&first_nft, alice.puzzle_hash));
    assert!(t.owns_nft(&second_nft, bob.puzzle_hash));
    assert_eq!(t.balance(carol.puzzle_hash), 20_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 1000);

    Ok(())
}

#[test]
fn test_aggregate_bids_different_prices() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(30_000))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 3000));
    assert_eq!(
        offer.requested_nft_trade_prices(first_nft.info.launcher_id),
        vec![xch_trade_price(10_000)]
    );
    assert_eq!(
        offer.requested_nft_trade_prices(second_nft.info.launcher_id),
        vec![xch_trade_price(30_000)]
    );

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert_eq!(t.balance(carol.puzzle_hash), 40_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);

    Ok(())
}

#[test]
fn test_extend_bids() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let first_nft = t.mint_nft(&carol, ROYALTY_A, 500)?;
    let second_nft = t.mint_nft(&carol, ROYALTY_B, 1000)?;
    let alice_offer = t.bid(&alice, &first_nft, Price::Xch(10_000))?;
    let bob_offer = t.bid(&bob, &second_nft, Price::Xch(30_000))?;

    let mut offer = t.aggregate(&[alice_offer])?;
    offer.extend(t.aggregate(&[bob_offer])?)?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(500 + 3000));
    assert_eq!(
        offer.requested_nft_trade_prices(first_nft.info.launcher_id),
        vec![xch_trade_price(10_000)]
    );
    assert_eq!(
        offer.requested_nft_trade_prices(second_nft.info.launcher_id),
        vec![xch_trade_price(30_000)]
    );

    t.take(&carol, &offer, |spends| {
        spends.add(first_nft);
        spends.add(second_nft);
    })?;

    assert!(t.owns_nft(&first_nft, alice.puzzle_hash));
    assert!(t.owns_nft(&second_nft, bob.puzzle_hash));
    assert_eq!(t.balance(carol.puzzle_hash), 40_000);
    assert_eq!(t.balance(ROYALTY_A), 500);
    assert_eq!(t.balance(ROYALTY_B), 3000);

    Ok(())
}

#[test]
fn test_unmatchable_prepaid_royalty_is_not_revealed() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 20_000)?;
    let offer = t.bid_with_royalty(&alice, &nft, Price::Xch(1000), 3)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(3));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 3);

    Ok(())
}

#[test]
fn test_prepaid_royalties_for_royalty_free_nft_are_not_revealed() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 0)?;
    let offer = t.bid_with_royalty(&alice, &nft, Price::Xch(1000), 5)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(5));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 5);

    Ok(())
}

#[test]
fn test_smallest_trade_price_for_prepaid_royalty() -> anyhow::Result<()> {
    let launcher_id = Bytes32::new([4; 32]);

    let trade_prices = |basis_points: u16, royalty: u64| -> anyhow::Result<Vec<TradePrice>> {
        let mut ctx = SpendContext::new();
        let mut offered_coins = OfferCoins::new();
        offered_coins.prepaid_royalties.xch = vec![
            RoyaltyInfo::new(launcher_id, ROYALTY_A, basis_points).payment(&mut ctx, royalty)?,
        ];

        let mut requested_payments = RequestedPayments::new();
        requested_payments.nfts.insert(launcher_id, Vec::new());

        let mut asset_info = AssetInfo::new();
        asset_info.insert_nft(
            launcher_id,
            NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, basis_points),
        )?;

        let offer = Offer::new(
            SpendBundle::new(Vec::new(), Signature::default()),
            offered_coins,
            requested_payments,
            asset_info,
        );

        Ok(offer.requested_nft_trade_prices(launcher_id))
    };

    assert_eq!(trade_prices(300, 30)?, vec![xch_trade_price(1000)]);
    assert_eq!(trade_prices(300, 31)?, vec![xch_trade_price(1034)]);
    assert_eq!(trade_prices(10_000, 7)?, vec![xch_trade_price(7)]);
    assert_eq!(trade_prices(20_000, 8)?, vec![xch_trade_price(4)]);

    assert_eq!(trade_prices(20_000, 7)?, Vec::new());
    assert_eq!(trade_prices(1, u64::MAX)?, Vec::new());

    for basis_points in [1, 3, 250, 300, 9999, 10_000, 10_001, 65_535] {
        for royalty in (1..=200).chain([u64::MAX / 7, u64::MAX]) {
            let trade_prices = trade_prices(basis_points, royalty)?;

            if basis_points <= 10_000 && royalty <= 200 {
                assert_eq!(trade_prices.len(), 1);
            }

            for trade_price in trade_prices {
                let amount = trade_price.amount;
                assert_eq!(
                    calculate_nft_royalty(amount, basis_points),
                    u128::from(royalty)
                );
                assert!(
                    amount == 0 || calculate_nft_royalty(amount - 1, basis_points) < royalty.into()
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_aggregate_sale_and_bid_for_same_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();
    let carol = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 500)?;
    let alice_offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let bob_offer = t.bid(&bob, &nft, Price::Xch(1500))?;
    let offer = t.aggregate(&[alice_offer, bob_offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(50));
    assert_eq!(offer.offered_royalty_amounts()?, xch(75));

    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        Vec::new()
    );

    t.take(&carol, &offer, |_| {})?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(carol.puzzle_hash), 450);
    assert_eq!(t.balance(ROYALTY_A), 50 + 75);

    Ok(())
}

#[test]
fn test_malformed_prepaid_royalties_are_ignored() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&bob, ROYALTY_A, 300)?;
    let launcher_id = nft.info.launcher_id;
    let royalty_hint = t.ctx.hint(ROYALTY_A)?;
    let other_hint = t.ctx.hint(ROYALTY_B)?;

    let malformed = vec![
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, 30, Memos::None)]),
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, 30, other_hint)]),
        NotarizedPayment::new(
            launcher_id,
            vec![
                Payment::new(ROYALTY_A, 30, royalty_hint),
                Payment::new(ROYALTY_B, 1, other_hint),
            ],
        ),
    ];

    let offer = t.bid_with_settlements(&alice, &nft, &[(Price::Xch(1000), malformed.clone())])?;
    let offer = t.aggregate(&[offer])?;

    assert!(offer.offered_coins().prepaid_royalties.xch.is_empty());
    assert_eq!(offer.offered_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 90);

    let nft = t.mint_nft(&bob, ROYALTY_B, 300)?;
    let mut settlements = vec![NotarizedPayment::new(
        nft.info.launcher_id,
        vec![Payment::new(ROYALTY_B, 30, Memos::None)],
    )];
    settlements
        .push(RoyaltyInfo::new(nft.info.launcher_id, ROYALTY_B, 300).payment(&mut t.ctx, 30)?);

    let offer = t.bid_with_settlements(&alice, &nft, &[(Price::Xch(1000), settlements)])?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.offered_royalty_amounts()?, xch(30));
    assert_eq!(
        offer.requested_nft_trade_prices(nft.info.launcher_id),
        vec![xch_trade_price(1000)]
    );

    t.take(&bob, &offer, |spends| spends.add(nft))?;

    assert!(t.owns_nft(&nft, alice.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_B), 1 + 30 + 30);

    Ok(())
}
