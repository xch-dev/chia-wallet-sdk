use super::*;

#[test]
fn test_payable_trade_prices() {
    let asset_id = Bytes32::new([9; 32]);
    let trade_prices = [
        xch_trade_price(10),
        xch_trade_price(34),
        cat_trade_price(asset_id, 1000),
    ];

    // 3% of 10 rounds down to zero, but 3% of 34 is 1
    assert_eq!(
        payable_trade_prices(&trade_prices, 300),
        vec![xch_trade_price(34), cat_trade_price(asset_id, 1000)]
    );
    assert_eq!(payable_trade_prices(&trade_prices, 0), Vec::new());
    assert_eq!(payable_trade_prices(&[], 300), Vec::new());
}

#[test]
fn test_sell_nft_for_xch() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(30));

    let coin = t.fund(&bob, 1030);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 1000);
    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_royalty_rounds_down() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // 3% of 1999 is 59.97
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1999))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(59));

    let coin = t.fund(&bob, 1999 + 59);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 59);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_royalty_must_match_exactly() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;
    let other_nonce = Bytes32::new([7; 32]);

    let mut wrong_royalties = Vec::new();

    for amount in [29, 31] {
        wrong_royalties.push(t.royalty(Id::Xch, &nft, ROYALTY_A, amount)?);
    }

    wrong_royalties.push(t.royalty(Id::Xch, &nft, ROYALTY_B, 30)?);

    let hint = t.ctx.hint(ROYALTY_A)?;
    wrong_royalties.push(Action::settle(
        Id::Xch,
        NotarizedPayment::new(other_nonce, vec![Payment::new(ROYALTY_A, 30, hint)]),
    ));
    wrong_royalties.push(Action::settle(
        Id::Xch,
        NotarizedPayment::new(
            nft.info.launcher_id,
            vec![Payment::new(ROYALTY_A, 30, Memos::None)],
        ),
    ));

    for royalty in wrong_royalties {
        let coin = t.fund(&bob, 1031);
        let error = t
            .take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                vec![royalty],
                |_, _| Vec::new(),
            )
            .expect_err("wrong royalty payment should be rejected");

        assert!(
            is_validation_error(&error, ErrorCode::AssertPuzzleAnnouncementFailed),
            "{error}"
        );
    }

    let coin = t.fund(&bob, 1030);
    let error = t
        .take_with(
            &bob,
            &offer,
            |spends| spends.add(coin),
            Vec::new(),
            |_, _| Vec::new(),
        )
        .expect_err("missing royalty payment should be rejected");

    assert!(is_validation_error(
        &error,
        ErrorCode::AssertPuzzleAnnouncementFailed
    ));

    Ok(())
}

#[test]
fn test_royalty_must_be_paid_in_trade_price_asset() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 1000)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    let coin = t.fund(&bob, 30);
    let royalty = t.royalty(Id::Xch, &nft, ROYALTY_A, 30)?;
    let error = t
        .take_with(
            &bob,
            &offer,
            |spends| {
                spends.add(coin);
                spends.add(bob_cat);
            },
            vec![royalty],
            |_, _| Vec::new(),
        )
        .expect_err("royalty paid in the wrong asset should be rejected");

    assert!(is_validation_error(
        &error,
        ErrorCode::AssertPuzzleAnnouncementFailed
    ));

    Ok(())
}

#[test]
fn test_sell_nft_for_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 1030)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, cat(asset_id, 30));

    t.take(&bob, &offer, |spends| spends.add(bob_cat))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.cat_balance(&bob_cat, alice.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_sell_nft_for_revocable_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // The hidden puzzle hash changes the CAT's settlement puzzle hash
    let bob_cat = t.issue_revocable_cat(&bob, 1030)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &cat(asset_id, 1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, cat(asset_id, 30));

    t.take(&bob, &offer, |spends| spends.add(bob_cat))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.cat_balance(&bob_cat, alice.puzzle_hash), 1000);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_sell_nft_for_xch_and_cat() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let bob_cat = t.issue_cat(&bob, 515)?;
    let asset_id = bob_cat.info.asset_id;
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let requested = OfferAmounts {
        xch: 1000,
        cats: indexmap! { asset_id => 500 },
    };
    let offer = t.sell_nfts(&alice, &[nft], &requested)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: 30,
            cats: indexmap! { asset_id => 15 },
        }
    );

    let coin = t.fund(&bob, 1030);
    t.take(&bob, &offer, |spends| {
        spends.add(coin);
        spends.add(bob_cat);
    })?;

    assert_eq!(t.balance(ROYALTY_A), 30);
    assert_eq!(t.cat_balance(&bob_cat, ROYALTY_A), 15);
    assert_eq!(t.balance(bob.puzzle_hash), 0);
    assert_eq!(t.cat_balance(&bob_cat, bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_each_trade_price_is_paid_separately() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 1000)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(400), |_, _| {
        vec![xch_trade_price(100), xch_trade_price(300)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(40));

    let coin = t.fund(&bob, 440);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    let royalty_coins = t.sim.unspent_coins(ROYALTY_A, false);
    let mut royalty_amounts: Vec<u64> = royalty_coins.iter().map(|coin| coin.amount).collect();
    royalty_amounts.sort_unstable();
    assert_eq!(royalty_amounts, vec![10, 30]);

    Ok(())
}

#[test]
fn test_identical_royalty_payments_are_paid_once() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    // 1% of both 100 and 101 is 1, so the NFT asserts the same announcement twice
    let nft = t.mint_nft(&alice, ROYALTY_A, 100)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(201), |_, _| {
        vec![xch_trade_price(100), xch_trade_price(101)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, xch(1));

    let coin = t.fund(&bob, 202);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert_eq!(t.balance(ROYALTY_A), 1);
    assert_eq!(t.balance(bob.puzzle_hash), 0);

    Ok(())
}

#[test]
fn test_nft_for_nft_has_no_royalties() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let alice_nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let bob_nft = t.mint_nft(&bob, ROYALTY_B, 500)?;

    let nonce = Offer::nonce(vec![alice_nft.coin.coin_id()]);
    let hint = t.ctx.hint(alice.puzzle_hash)?;
    let mut requested_payments = RequestedPayments::new();
    requested_payments.nfts.insert(
        bob_nft.info.launcher_id,
        vec![NotarizedPayment::new(
            nonce,
            vec![Payment::new(alice.puzzle_hash, 1, hint)],
        )],
    );
    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        bob_nft.info.launcher_id,
        NftAssetInfo::new(
            bob_nft.info.metadata,
            bob_nft.info.metadata_updater_puzzle_hash,
            bob_nft.info.royalty_puzzle_hash,
            bob_nft.info.royalty_basis_points,
        ),
    )?;

    let mut spends = Spends::new(alice.puzzle_hash);
    spends.add(alice_nft);
    spends.conditions.required = spends
        .conditions
        .required
        .extend(requested_payments.assertions(&mut t.ctx, &asset_info)?);
    let id = Id::Existing(alice_nft.info.launcher_id);
    let (_, coin_spends) = t.build(
        spends,
        &[
            Action::update_nft(id, vec![], Some(TransferNftById::new(None, Vec::new()))),
            Action::send(id, SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None),
        ],
        Relation::AssertConcurrent,
        &alice,
    )?;
    let offer = t.finish_offer(&alice, coin_spends, requested_payments, asset_info)?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert_eq!(offer.offered_royalty_amounts()?, OfferAmounts::new());
    assert!(
        offer
            .requested_royalty_payments(&mut t.ctx)?
            .actions()
            .is_empty()
    );
    assert_eq!(
        offer.requested_nft_trade_prices(bob_nft.info.launcher_id),
        Vec::new()
    );

    t.take(&bob, &offer, |spends| spends.add(bob_nft))?;

    assert!(t.owns_nft(&alice_nft, bob.puzzle_hash));
    assert!(t.owns_nft(&bob_nft, alice.puzzle_hash));

    Ok(())
}

#[test]
fn test_royalty_free_nft() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 0)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    let coin = t.fund(&bob, 1000);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(ROYALTY_A), 0);

    Ok(())
}

#[test]
fn test_zero_royalty_trade_price_is_unpayable() -> anyhow::Result<()> {
    for (basis_points, price) in [(300, 10), (0, 1000)] {
        let mut t = Tester::new();
        let alice = t.pair();
        let bob = t.pair();

        let nft = t.mint_nft(&alice, ROYALTY_A, basis_points)?;
        let offer = t.sell_nfts_with(&alice, &[nft], &xch(price), |_, _| {
            vec![xch_trade_price(price)]
        })?;
        let offer = t.aggregate(&[offer])?;

        assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
        assert!(matches!(
            offer.requested_royalty_payments(&mut t.ctx),
            Err(DriverError::ZeroRoyaltyPayment(launcher_id)) if launcher_id == nft.info.launcher_id
        ));

        // The NFT asserts a payment of zero, which settlement refuses to make
        let coin = t.fund(&bob, price);
        let hint = t.ctx.hint(ROYALTY_A)?;
        let zero_royalty = Action::settle(
            Id::Xch,
            NotarizedPayment::new(nft.info.launcher_id, vec![Payment::new(ROYALTY_A, 0, hint)]),
        );
        assert!(
            t.take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                vec![zero_royalty],
                |_, _| Vec::new(),
            )
            .is_err()
        );

        let coin = t.fund(&bob, price);
        let error = t
            .take_with(
                &bob,
                &offer,
                |spends| spends.add(coin),
                Vec::new(),
                |_, _| Vec::new(),
            )
            .expect_err("the royalty announcement can never be made");
        assert!(is_validation_error(
            &error,
            ErrorCode::AssertPuzzleAnnouncementFailed
        ));
    }

    Ok(())
}

#[test]
fn test_payable_trade_prices_make_small_sales_takeable() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let bob = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(10))?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());

    let coin = t.fund(&bob, 10);
    t.take(&bob, &offer, |spends| spends.add(coin))?;

    assert!(t.owns_nft(&nft, bob.puzzle_hash));
    assert_eq!(t.balance(alice.puzzle_hash), 10);

    Ok(())
}

#[test]
fn test_unknown_trade_price_asset_is_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let unknown_asset_id = Bytes32::new([8; 32]);
    let unknown = cat_trade_price(unknown_asset_id, 1000);
    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1000), |_, _| vec![unknown])?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::UnknownTradePriceAsset(puzzle_hash)) if puzzle_hash == unknown.puzzle_hash
    ));

    Ok(())
}
