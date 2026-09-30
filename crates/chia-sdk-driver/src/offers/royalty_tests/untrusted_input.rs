use super::*;

#[test]
fn test_prepaid_royalties_from_untrusted_offer() -> anyhow::Result<()> {
    let launcher_id = Bytes32::new([4; 32]);
    let hint = Memos::None;

    let mut offered_coins = OfferCoins::new();
    offered_coins.settled_payments.xch = vec![
        NotarizedPayment::new(
            launcher_id,
            vec![
                Payment::new(ROYALTY_A, u64::MAX, hint),
                Payment::new(ROYALTY_B, 1000, hint),
            ],
        ),
        NotarizedPayment::new(launcher_id, vec![Payment::new(ROYALTY_A, u64::MAX, hint)]),
        NotarizedPayment::new(
            Bytes32::new([5; 32]),
            vec![Payment::new(ROYALTY_A, 1000, hint)],
        ),
    ];

    // A requested NFT without asset info has no known royalty address
    let mut requested_payments = RequestedPayments::new();
    for id in [launcher_id, Bytes32::new([6; 32])] {
        requested_payments.nfts.insert(
            id,
            vec![NotarizedPayment::new(
                Bytes32::default(),
                vec![Payment::new(ROYALTY_C, 1, hint)],
            )],
        );
    }

    let mut asset_info = AssetInfo::new();
    asset_info.insert_nft(
        launcher_id,
        NftAssetInfo::new(HashedPtr::NIL, Bytes32::default(), ROYALTY_A, 1),
    )?;

    let offer = Offer::new(
        SpendBundle::new(Vec::new(), Signature::default()),
        offered_coins,
        requested_payments,
        asset_info,
    );

    // Only payments to the royalty address with the NFT's nonce count, and the total can exceed a
    // u64
    assert_eq!(
        offer.offered_royalty_amounts()?,
        OfferAmounts {
            xch: 2 * u128::from(u64::MAX),
            cats: IndexMap::new(),
        }
    );

    // No trade price is large enough to produce these royalties
    assert_eq!(offer.requested_nft_trade_prices(launcher_id), Vec::new());
    assert_eq!(
        offer.requested_nft_trade_prices(Bytes32::new([6; 32])),
        Vec::new()
    );

    Ok(())
}

#[test]
fn test_unparseable_settled_payments_are_ignored() -> anyhow::Result<()> {
    let mut t = Tester::new();
    let alice = t.pair();
    let settlement = t.ctx.alloc_mod::<SettlementPayment>()?;

    // Settlement accepts a nonce that isn't 32 bytes, but it can't be parsed as a payment
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

    assert!(offer.offered_coins().settled_payments.xch.is_empty());
    assert!(offer.offered_coins().settled_payments.cats.is_empty());

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

    // The NFT's solution doesn't match its layers
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

    // The NFT's inner puzzle raises
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

    // Royalties above 100% are allowed, so a trade price that fits in a coin can have a royalty
    // that doesn't
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
