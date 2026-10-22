use super::*;

#[test]
fn test_prepaid_royalties_from_untrusted_offer() -> anyhow::Result<()> {
    let mut ctx = SpendContext::new();
    let launcher_id = Bytes32::new([4; 32]);
    let royalty = RoyaltyInfo::new(launcher_id, ROYALTY_A, 1);

    let mut offered_coins = OfferCoins::new();
    offered_coins.prepaid_royalties.xch = vec![
        royalty.payment(&mut ctx, u64::MAX)?,
        royalty.payment(&mut ctx, u64::MAX)?,
        RoyaltyInfo::new(launcher_id, ROYALTY_B, 1).payment(&mut ctx, 1000)?,
        RoyaltyInfo::new(Bytes32::new([5; 32]), ROYALTY_A, 1).payment(&mut ctx, 1000)?,
    ];

    let mut requested_payments = RequestedPayments::new();
    requested_payments.nfts.insert(
        launcher_id,
        vec![NotarizedPayment::new(
            Bytes32::default(),
            vec![Payment::new(ROYALTY_C, 1, Memos::None)],
        )],
    );

    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        launcher_id,
        NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, 1),
    )?;

    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins.clone(),
        requested_payments.clone(),
        asset_info.clone(),
    );

    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 2 * u128::from(u64::MAX),
            cats: IndexMap::new(),
        }
    );

    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());

    let unknown_launcher_id = Bytes32::new([6; 32]);
    requested_payments
        .nfts
        .insert(unknown_launcher_id, Vec::new());
    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins,
        requested_payments,
        asset_info,
    );

    assert!(matches!(
        offer.offered_royalty_amounts(),
        Err(DriverError::MissingAssetInfo)
    ));
    assert_eq!(
        offer.requested_nft_trade_prices(unknown_launcher_id),
        Vec::new()
    );

    Ok(())
}

#[test]
fn test_unparseable_prepaid_royalties_are_ignored() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let settlement = t.ctx.alloc_mod::<SettlementPayment>()?;

    let solution = t.ctx.alloc(&clvm_list!(clvm_list!(1)))?;
    let xch_spend = CoinSpend::new(
        Coin::new(Bytes32::new([1; 32]), SETTLEMENT_PAYMENT_HASH.into(), 1),
        t.ctx.serialize(&settlement)?,
        t.ctx.serialize(&solution)?,
    );

    let cat = t
        .issue_cat(&alice, 1)?
        .child(SETTLEMENT_PAYMENT_HASH.into(), 1);
    let solution = t
        .ctx
        .alloc(&clvm_list!(clvm_list!(1, clvm_list!(ROYALTY_A, 1))))?;
    Cat::spend_all(
        &mut t.ctx,
        &[CatSpend::new(cat, Spend::new(settlement, solution))],
    )?;

    let mut coin_spends = t.ctx.take();
    coin_spends.push(xch_spend);
    let offer = Offer::from_spend_bundle(
        &mut t.ctx,
        &SpendBundle::new(coin_spends, Signature::default()),
    )?;

    assert!(offer.offered_coins().prepaid_royalties.xch.is_empty());
    assert!(offer.offered_coins().prepaid_royalties.cats.is_empty());

    Ok(())
}

#[test]
fn test_invalid_nft_spends_are_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let raise = t.ctx.alloc(&clvm_list!(8))?;

    let spend_nft = |ctx: &mut SpendContext, inner_puzzle| -> anyhow::Result<()> {
        let spend = nft.info.into_layers(inner_puzzle).construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: nft.proof,
                amount: nft.coin.amount,
                inner_solution: NftStateLayerSolution {
                    inner_solution: NftOwnershipLayerSolution {
                        inner_solution: NodePtr::NIL,
                    },
                },
            },
        )?;
        ctx.spend(nft.coin, spend)?;
        Ok(())
    };

    spend_nft(&mut t.ctx, raise)?;
    let mut coin_spends = t.ctx.take();
    coin_spends[0].solution = t.ctx.serialize(&1)?;
    assert!(
        Offer::from_spend_bundle(
            &mut t.ctx,
            &SpendBundle::new(coin_spends, Signature::default())
        )
        .is_err()
    );

    spend_nft(&mut t.ctx, raise)?;
    let coin_spends = t.ctx.take();
    assert!(
        Offer::from_spend_bundle(
            &mut t.ctx,
            &SpendBundle::new(coin_spends, Signature::default())
        )
        .is_err()
    );

    Ok(())
}

#[test]
fn test_offered_nft_without_known_trade_prices() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, 300)?;
    let offer = t.sell_nfts(&alice, &[nft], &xch(1000))?;
    let offer = t.aggregate(&[offer])?;

    let mut offered_coins = offer.offered_coins().clone();
    offered_coins.nft_trade_prices.clear();
    let offer = Offer::new(
        offer.spend_bundle().clone(),
        offered_coins,
        offer.requested_payments().clone(),
        offer.asset_info().clone(),
    );

    assert_eq!(offer.requested_royalty_amounts()?, OfferAmounts::new());
    assert!(
        offer
            .requested_royalty_payments(&mut t.ctx)?
            .actions()
            .is_empty()
    );

    Ok(())
}

#[test]
fn test_royalty_larger_than_a_coin_is_rejected() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();

    let nft = t.mint_nft(&alice, ROYALTY_A, u16::MAX)?;
    let offer = t.sell_nfts_with(&alice, &[nft], &xch(1), |_, _| {
        vec![xch_trade_price(u64::MAX)]
    })?;
    let offer = t.aggregate(&[offer])?;

    assert_eq!(
        offer.requested_royalty_amounts()?,
        OfferAmounts {
            xch: u128::from(u64::MAX) * 65_535 / 10_000,
            cats: IndexMap::new(),
        }
    );
    assert!(matches!(
        offer.requested_royalty_payments(&mut t.ctx),
        Err(DriverError::AmountOverflow)
    ));

    Ok(())
}
