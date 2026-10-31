use anyhow::Result;
use chia_bls::Signature;
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::{
    Memos,
    nft::{NftMetadata, NftOwnershipLayerSolution, NftStateLayerSolution},
};
use chia_sdk_test::{BlsPairWithCoin, Simulator, SimulatorError, sign_transaction};
use chia_sdk_types::{
    Condition, announcement_id, conditions::AssertPuzzleAnnouncement, payment_assertion,
    tree_hash_notarized_payment,
};
use clvm_traits::clvm_list;
use hex_literal::hex;
use rstest::rstest;

use crate::{AssetInfo, NftAssetInfo, NftMint, RequestedPayments, StandardLayer};

use super::*;

const ATTACKER: Bytes32 = Bytes32::new([0xba; 32]);

struct Fixture {
    sim: Simulator,
    ctx: SpendContext,
    user: BlsPairWithCoin,
    fusion: Fusion,
    side_a: Vec<Nft>,
    side_b: Vec<Nft>,
}

fn mint_nfts(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    owner: &BlsPairWithCoin,
    count: usize,
    royalty_basis_points: u16,
) -> Result<Vec<Nft>> {
    let p2 = StandardLayer::new(owner.pk);
    let mut nfts = Vec::new();

    for edition_number in 0..count {
        let coin = sim.new_coin(owner.puzzle_hash, 1);
        let metadata = ctx.alloc_hashed(&NftMetadata {
            edition_number: edition_number as u64 + 1,
            ..Default::default()
        })?;
        let mint = NftMint::new(metadata, owner.puzzle_hash, royalty_basis_points, None);
        let (conditions, nft) = Launcher::new(coin.coin_id(), 1).mint_nft(ctx, &mint)?;
        p2.spend(ctx, coin, conditions)?;
        nfts.push(nft);
    }

    sim.spend_coins(ctx.take(), std::slice::from_ref(&owner.sk))?;

    Ok(nfts)
}

fn launch(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    owner: &BlsPairWithCoin,
    side_a: &[Nft],
    side_b: &[Nft],
) -> Result<Fusion> {
    let launcher_ids = |nfts: &[Nft]| nfts.iter().map(|nft| nft.info.launcher_id).collect();
    let coin = sim.new_coin(owner.puzzle_hash, 1);
    let (conditions, fusion) = Launcher::for_fusion(ctx, coin.coin_id())?.launch_fusion(
        ctx,
        launcher_ids(side_a),
        launcher_ids(side_b),
    )?;
    StandardLayer::new(owner.pk).spend(ctx, coin, conditions)?;
    Ok(fusion)
}

/// Mints both sides and deploys a fusion with side A locked. The user holds side B.
fn fixture(a_count: usize, b_count: usize, royalty_basis_points: u16) -> Result<Fixture> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let house = sim.bls(0);
    let user = sim.bls(0);

    let side_a = mint_nfts(&mut sim, &mut ctx, &house, a_count, royalty_basis_points)?;
    let side_b = mint_nfts(&mut sim, &mut ctx, &user, b_count, royalty_basis_points)?;

    let fusion = launch(&mut sim, &mut ctx, &house, &side_a, &side_b)?;

    let p2 = StandardLayer::new(house.pk);
    let side_a = side_a
        .into_iter()
        .map(|nft| fusion.lock_nft(&mut ctx, nft, &p2, Conditions::new()))
        .collect::<Result<_, _>>()?;

    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    Ok(Fixture {
        sim,
        ctx,
        user,
        fusion,
        side_a,
        side_b,
    })
}

/// The maker offers NFTs in exchange for NFTs sent to `recipient`, as a wallet would.
fn make_offer(
    ctx: &mut SpendContext,
    maker: &BlsPairWithCoin,
    offered: &[Nft],
    requested: &[Nft],
    recipient: Bytes32,
) -> Result<Offer> {
    let nonce = Offer::nonce(offered.iter().map(|nft| nft.coin.coin_id()).collect());

    let mut requested_payments = RequestedPayments::new();
    let mut asset_info = AssetInfo::new();

    for nft in requested {
        let memos = ctx.hint(recipient)?;
        requested_payments.nfts.insert(
            nft.info.launcher_id,
            vec![NotarizedPayment::new(
                nonce,
                vec![Payment::new(recipient, 1, memos)],
            )],
        );
        asset_info.insert_nft(
            nft.info.launcher_id,
            NftAssetInfo::new(
                nft.info.metadata,
                nft.info.metadata_updater_puzzle_hash,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            ),
        )?;
    }

    let assertions = requested_payments.assertions(ctx, &asset_info)?;
    let p2 = StandardLayer::new(maker.pk);

    for nft in offered {
        let _ = nft.lock_settlement(
            ctx,
            &p2,
            Vec::new(),
            Conditions::new().extend(assertions.clone()),
        )?;
    }

    let coin_spends = ctx.take();
    let signature = sign_transaction(&coin_spends, std::slice::from_ref(&maker.sk))?;

    Ok(Offer::from_input_spend_bundle(
        ctx,
        SpendBundle::new(coin_spends, signature),
        requested_payments,
        asset_info,
    )?)
}

/// The assertion that makes a spend depend on the NFT being paid to the recipient, which is what
/// protects a spend into the settlement puzzle when there's no offer.
fn assert_payment(
    ctx: &mut SpendContext,
    nft: &Nft,
    nonce: Bytes32,
    recipient: Bytes32,
) -> Result<AssertPuzzleAnnouncement> {
    let payment = settlement_payment(ctx, nonce, recipient)?;
    let settlement = NftInfo {
        p2_puzzle_hash: SETTLEMENT_PAYMENT_HASH.into(),
        ..nft.info
    };
    Ok(payment_assertion(
        settlement.puzzle_hash().into(),
        tree_hash_notarized_payment(ctx, &payment),
    ))
}

fn assert_swap(sim: &Simulator, fusion: &FusionSwap, recipient: Bytes32, p2: Bytes32) {
    assert!(sim.coin_state(fusion.fusion.coin.coin_id()).is_some());

    for nft in &fusion.unlocked {
        assert_eq!(nft.info.p2_puzzle_hash, recipient);
        let state = sim.coin_state(nft.coin.coin_id()).expect("unlocked NFT");
        assert_eq!(state.spent_height, None);
    }

    for nft in &fusion.locked {
        assert_eq!(nft.info.p2_puzzle_hash, p2);
        let state = sim.coin_state(nft.coin.coin_id()).expect("locked NFT");
        assert_eq!(state.spent_height, None);
    }
}

fn take(
    sim: &mut Simulator,
    offer: Offer,
    coin_spends: Vec<CoinSpend>,
) -> Result<(), SimulatorError> {
    sim.new_transaction(offer.take(SpendBundle::new(coin_spends, Signature::default())))?;
    Ok(())
}

/// Fusing, defusing and fusing again, with NFTs on either side. An upgrade is one NFT for one NFT,
/// and dividing an NFT is fusing with more NFTs on side A.
#[rstest]
#[case::upgrade(1, 1)]
#[case::fuse_two(1, 2)]
#[case::fuse_three(1, 3)]
#[case::fuse_four(1, 4)]
#[case::divide(2, 1)]
#[case::many_to_many(2, 3)]
fn test_fuse_and_defuse(#[case] a_count: usize, #[case] b_count: usize) -> Result<()> {
    let Fixture {
        mut sim,
        mut ctx,
        user,
        fusion,
        side_a,
        side_b,
    } = fixture(a_count, b_count, 0)?;
    let ctx = &mut ctx;
    let p2 = fusion.info.p2_puzzle_hash();

    // The user offers side B for side A
    let offer = make_offer(ctx, &user, &side_b, &side_a, user.puzzle_hash)?;
    let fused = fusion.take_offer(ctx, &offer, &side_a)?;
    take(&mut sim, offer, ctx.take())?;
    assert_swap(&sim, &fused, user.puzzle_hash, p2);

    // Then offers side A to get side B back
    let offer = make_offer(ctx, &user, &fused.unlocked, &fused.locked, user.puzzle_hash)?;
    let defused = fused.fusion.take_offer(ctx, &offer, &fused.locked)?;
    take(&mut sim, offer, ctx.take())?;
    assert_swap(&sim, &defused, user.puzzle_hash, p2);

    // And it can be done any number of times
    let offer = make_offer(
        ctx,
        &user,
        &defused.unlocked,
        &defused.locked,
        user.puzzle_hash,
    )?;
    let refused = defused.fusion.take_offer(ctx, &offer, &defused.locked)?;
    take(&mut sim, offer, ctx.take())?;
    assert_swap(&sim, &refused, user.puzzle_hash, p2);

    Ok(())
}

#[test]
fn test_royalties_are_not_charged_for_swaps() -> Result<()> {
    let mut fx = fixture(1, 1, 500)?;
    let ctx = &mut fx.ctx;

    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, fx.user.puzzle_hash)?;
    let swap = fx.fusion.take_offer(ctx, &offer, &fx.side_a)?;
    take(&mut fx.sim, offer, ctx.take())?;
    assert_swap(
        &fx.sim,
        &swap,
        fx.user.puzzle_hash,
        fx.fusion.info.p2_puzzle_hash(),
    );

    let royalty_puzzle_hash = fx.side_a[0].info.royalty_puzzle_hash;
    let royalties: u64 = fx
        .sim
        .unspent_coins(royalty_puzzle_hash, false)
        .iter()
        .map(|coin| coin.amount)
        .sum();
    assert_eq!(royalties, 0);

    Ok(())
}

#[test]
fn test_take_offer_from_untrusted_bundle() -> Result<()> {
    let mut fx = fixture(1, 2, 0)?;
    let ctx = &mut fx.ctx;

    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, fx.user.puzzle_hash)?;
    let spend_bundle = offer.to_spend_bundle(ctx)?;
    let offer = Offer::from_spend_bundle(ctx, &spend_bundle)?;

    let swap = fx.fusion.take_offer(ctx, &offer, &fx.side_a)?;
    take(&mut fx.sim, offer, ctx.take())?;
    assert_swap(
        &fx.sim,
        &swap,
        fx.user.puzzle_hash,
        fx.fusion.info.p2_puzzle_hash(),
    );

    Ok(())
}

/// The NFTs must be in the curried order, so the driver has to sort them.
#[test]
fn test_order_of_nfts_does_not_matter() -> Result<()> {
    let mut fx = fixture(2, 3, 0)?;
    let ctx = &mut fx.ctx;

    let mut offered = fx.side_b.clone();
    offered.reverse();
    let mut requested = fx.side_a.clone();
    requested.reverse();

    let offer = make_offer(ctx, &fx.user, &offered, &requested, fx.user.puzzle_hash)?;
    let swap = fx.fusion.take_offer(ctx, &offer, &requested)?;
    take(&mut fx.sim, offer, ctx.take())?;
    assert_swap(
        &fx.sim,
        &swap,
        fx.user.puzzle_hash,
        fx.fusion.info.p2_puzzle_hash(),
    );

    Ok(())
}

/// Unlike what CHIP-0021 says, an offer file isn't needed, only NFTs in the settlement puzzle. The
/// spend into settlement still has to assert what it gets back, as [`test_direct_swap_needs_assertion`]
/// shows.
#[test]
fn test_swap_without_offer_file() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let nonce = Bytes32::new([42; 32]);
    let assertion = assert_payment(ctx, &fx.side_a[0], nonce, fx.user.puzzle_hash)?;
    let settled = fx.side_b[0].lock_settlement(
        ctx,
        &StandardLayer::new(fx.user.pk),
        Vec::new(),
        Conditions::new().with(assertion),
    )?;

    let swap = fx.fusion.swap(
        ctx,
        FusionDirection::Fuse,
        &[settled],
        &[FusionUnlock::new(fx.side_a[0], fx.user.puzzle_hash)],
        nonce,
    )?;

    fx.sim
        .spend_coins(ctx.take(), std::slice::from_ref(&fx.user.sk))?;
    assert_swap(
        &fx.sim,
        &swap,
        fx.user.puzzle_hash,
        fx.fusion.info.p2_puzzle_hash(),
    );

    Ok(())
}

/// Without an assertion, anyone can copy the signed spend into the settlement puzzle and pay the NFT
/// to themselves. With one, the spend is only valid if the user gets the NFT they asked for.
#[rstest]
#[case::unprotected(false)]
#[case::asserted(true)]
fn test_direct_swap_needs_assertion(#[case] asserted: bool) -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let nonce = Bytes32::new([42; 32]);
    let conditions = if asserted {
        let assertion = assert_payment(ctx, &fx.side_a[0], nonce, fx.user.puzzle_hash)?;
        Conditions::new().with(assertion)
    } else {
        Conditions::new()
    };

    let settled = fx.side_b[0].lock_settlement(
        ctx,
        &StandardLayer::new(fx.user.pk),
        Vec::new(),
        conditions,
    )?;

    // The attacker reuses the user's signed spend, and pays the NFT to themselves
    let payment = settlement_payment(ctx, nonce, ATTACKER)?;
    let stolen = settled.unlock_settlement(ctx, vec![payment])?;

    let result = fx
        .sim
        .spend_coins(ctx.take(), std::slice::from_ref(&fx.user.sk));

    if asserted {
        assert!(matches!(
            result,
            Err(SimulatorError::Validation(
                ErrorCode::AssertPuzzleAnnouncementFailed
            ))
        ));
    } else {
        result?;
        assert_eq!(stolen.info.p2_puzzle_hash, ATTACKER);
        assert!(fx.sim.coin_state(stolen.coin.coin_id()).is_some());
    }

    Ok(())
}

/// The puzzle commits to the owner, so an NFT which kept its owner can be swapped without an offer.
#[test]
fn test_swap_nft_with_assigned_owner() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(2);
    let user = sim.bls(0);
    let p2 = StandardLayer::new(house.pk);

    let (create_did, did) = Launcher::new(house.coin.coin_id(), 1).create_simple_did(ctx, &p2)?;
    p2.spend(ctx, house.coin, create_did)?;

    let metadata = ctx.alloc_hashed(&NftMetadata::default())?;
    let mint = NftMint::new(
        metadata,
        house.puzzle_hash,
        0,
        Some(TransferNft::new(
            Some(did.info.launcher_id),
            Vec::new(),
            Some(did.info.inner_puzzle_hash().into()),
        )),
    );
    let (mint_nft, nft_a) = crate::IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
        .create(ctx)?
        .mint_nft(ctx, &mint)?;
    let _did = did.update(ctx, &p2, mint_nft)?;
    assert!(nft_a.info.current_owner.is_some());
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    let side_b = mint_nfts(&mut sim, ctx, &user, 1, 0)?;
    let fusion = launch(&mut sim, ctx, &house, &[nft_a], &side_b)?;

    let locked = nft_a.transfer(ctx, &p2, fusion.info.p2_puzzle_hash(), Conditions::new())?;
    assert!(locked.info.current_owner.is_some());
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    // The maker's assertion would be for the NFT without an owner, so an offer can't be taken
    let offer = make_offer(ctx, &user, &side_b, &[locked], user.puzzle_hash)?;
    assert!(matches!(
        fusion.take_offer(ctx, &offer, &[locked]),
        Err(DriverError::FusionOfferMismatch)
    ));

    // But it can be swapped directly
    let nonce = Bytes32::new([42; 32]);
    let assertion = assert_payment(ctx, &locked, nonce, user.puzzle_hash)?;
    let settled = side_b[0].lock_settlement(
        ctx,
        &StandardLayer::new(user.pk),
        Vec::new(),
        Conditions::new().with(assertion),
    )?;
    let swap = fusion.swap(
        ctx,
        FusionDirection::Fuse,
        &[settled],
        &[FusionUnlock::new(locked, user.puzzle_hash)],
        nonce,
    )?;
    assert_eq!(
        swap.unlocked[0].info.current_owner,
        locked.info.current_owner
    );
    sim.spend_coins(ctx.take(), std::slice::from_ref(&user.sk))?;
    assert_swap(&sim, &swap, user.puzzle_hash, fusion.info.p2_puzzle_hash());

    Ok(())
}

/// A DID assigned to the NFT would stop it from being requested by an offer, so it's removed.
#[test]
fn test_lock_nft_removes_owner() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(2);
    let user = sim.bls(0);
    let p2 = StandardLayer::new(house.pk);

    let (create_did, did) = Launcher::new(house.coin.coin_id(), 1).create_simple_did(ctx, &p2)?;
    p2.spend(ctx, house.coin, create_did)?;

    let metadata = ctx.alloc_hashed(&NftMetadata::default())?;
    let mint = NftMint::new(
        metadata,
        house.puzzle_hash,
        0,
        Some(TransferNft::new(
            Some(did.info.launcher_id),
            Vec::new(),
            Some(did.info.inner_puzzle_hash().into()),
        )),
    );
    let (mint_nft, nft_a) = crate::IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
        .create(ctx)?
        .mint_nft(ctx, &mint)?;
    let _did = did.update(ctx, &p2, mint_nft)?;
    assert!(nft_a.info.current_owner.is_some());
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    let side_b = mint_nfts(&mut sim, ctx, &user, 1, 0)?;
    let fusion = launch(&mut sim, ctx, &house, &[nft_a], &side_b)?;
    let locked = fusion.lock_nft(ctx, nft_a, &p2, Conditions::new())?;
    assert_eq!(locked.info.current_owner, None);
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    let offer = make_offer(ctx, &user, &side_b, &[locked], user.puzzle_hash)?;
    let swap = fusion.take_offer(ctx, &offer, &[locked])?;
    take(&mut sim, offer, ctx.take())?;
    assert_swap(&sim, &swap, user.puzzle_hash, fusion.info.p2_puzzle_hash());

    Ok(())
}

#[test]
fn test_hints_and_parsing() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(0);
    let user = sim.bls(0);
    let side_a = mint_nfts(&mut sim, ctx, &house, 1, 0)?;
    let side_b = mint_nfts(&mut sim, ctx, &user, 2, 0)?;
    let launcher_coin_parent;

    let fusion = {
        launcher_coin_parent = sim.new_coin(house.puzzle_hash, 1);
        let launcher = Launcher::for_fusion(ctx, launcher_coin_parent.coin_id())?;
        let (conditions, fusion) = launcher.launch_fusion(
            ctx,
            vec![side_a[0].info.launcher_id],
            side_b.iter().map(|nft| nft.info.launcher_id).collect(),
        )?;
        let p2 = StandardLayer::new(house.pk);
        p2.spend(ctx, launcher_coin_parent, conditions)?;
        fusion
    };

    let locked = fusion.lock_nft(
        ctx,
        side_a[0],
        &StandardLayer::new(house.pk),
        Conditions::new(),
    )?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    // The launcher is hinted, and it can be parsed to recover the fusion and its sides
    let launcher_id = fusion.info.launcher_id;
    assert_eq!(sim.hinted_coins(FUSION_HINT), vec![launcher_id]);

    let launcher_spend = sim.coin_spend(launcher_id).expect("launcher spend");
    let solution = ctx.alloc(&launcher_spend.solution)?;
    assert_eq!(
        Fusion::parse_launcher(ctx, launcher_spend.coin, solution)?,
        Some(fusion.clone())
    );

    // The next singleton is hinted too, and the parent spend tells you which side is locked
    let offer = make_offer(ctx, &user, &side_b, &[locked], user.puzzle_hash)?;
    let swap = fusion.take_offer(ctx, &offer, &[locked])?;
    take(&mut sim, offer, ctx.take())?;

    assert!(
        sim.hinted_coins(FUSION_HINT)
            .contains(&swap.fusion.coin.coin_id())
    );

    let spend = sim.coin_spend(fusion.coin.coin_id()).expect("fusion spend");
    let puzzle = ctx.alloc(&spend.puzzle_reveal)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    let solution = ctx.alloc(&spend.solution)?;
    assert_eq!(
        Fusion::parse_child(ctx, spend.coin, puzzle, solution)?,
        Some((swap.fusion.clone(), FusionDirection::Fuse))
    );

    // The parsed child can be used for the next swap
    let unlocked = swap.unlocked;
    let offer = make_offer(ctx, &user, &unlocked, &swap.locked, user.puzzle_hash)?;
    let swap = swap.fusion.take_offer(ctx, &offer, &swap.locked)?;
    take(&mut sim, offer, ctx.take())?;

    let spend = sim
        .coin_spend(swap.fusion.coin.parent_coin_info)
        .expect("fusion spend");
    let puzzle = ctx.alloc(&spend.puzzle_reveal)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    let solution = ctx.alloc(&spend.solution)?;
    assert_eq!(
        Fusion::parse_child(ctx, spend.coin, puzzle, solution)?,
        Some((swap.fusion.clone(), FusionDirection::Defuse))
    );

    Ok(())
}

#[test]
fn test_parse_not_a_fusion() -> Result<()> {
    let ctx = &mut SpendContext::new();

    let puzzle = ctx.alloc(&1)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    assert_eq!(
        Fusion::parse_child(
            ctx,
            Coin::new(Bytes32::default(), Bytes32::default(), 1),
            puzzle,
            NodePtr::NIL
        )?,
        None
    );

    let launcher_coin = Coin::new(Bytes32::default(), SINGLETON_LAUNCHER_HASH.into(), 1);
    let launcher_id = launcher_coin.coin_id();
    let nft_a = Bytes32::new([1; 32]);
    let nft_b = Bytes32::new([2; 32]);
    let puzzle_hash: Bytes32 = FusionLayer::new(launcher_id, vec![nft_a], vec![nft_b])?
        .puzzle_hash()
        .into();

    // A launcher that doesn't have a memo
    let solution = ctx.alloc(&clvm_list!(puzzle_hash, 1, ()))?;
    assert_eq!(Fusion::parse_launcher(ctx, launcher_coin, solution)?, None);

    // A memo which doesn't match the launched puzzle hash
    let solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        clvm_list!(launcher_id, vec![nft_b], vec![nft_a])
    ))?;
    assert_eq!(Fusion::parse_launcher(ctx, launcher_coin, solution)?, None);

    // A memo for another launcher
    let solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        clvm_list!(Bytes32::default(), vec![nft_a], vec![nft_b])
    ))?;
    assert_eq!(Fusion::parse_launcher(ctx, launcher_coin, solution)?, None);

    // A coin that isn't a launcher
    let solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        clvm_list!(launcher_id, vec![nft_a], vec![nft_b])
    ))?;
    let coin = Coin::new(Bytes32::default(), Bytes32::default(), 1);
    assert_eq!(Fusion::parse_launcher(ctx, coin, solution)?, None);

    // The key value list on mainnet
    assert!(Fusion::parse_launcher(ctx, launcher_coin, solution)?.is_some());

    // The extra item used by the current version of the reference implementation
    let solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        (),
        clvm_list!(launcher_id, vec![nft_a], vec![nft_b])
    ))?;
    assert!(Fusion::parse_launcher(ctx, launcher_coin, solution)?.is_some());

    Ok(())
}

#[test]
fn test_lock_rejects_nfts_that_would_be_stuck() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;
    let p2 = StandardLayer::new(fx.user.pk);

    let foreign = mint_nfts(&mut fx.sim, ctx, &fx.user, 1, 0)?[0];
    assert!(matches!(
        fx.fusion.lock_nft(ctx, foreign, &p2, Conditions::new()),
        Err(DriverError::FusionNftMismatch)
    ));

    let mut custom_updater = fx.side_b[0];
    custom_updater.info.metadata_updater_puzzle_hash = Bytes32::default();
    assert!(matches!(
        fx.fusion
            .lock_nft(ctx, custom_updater, &p2, Conditions::new()),
        Err(DriverError::UnsupportedFusionNft)
    ));

    let mut wrong_amount = fx.side_b[0];
    wrong_amount.coin.amount = 3;
    assert!(matches!(
        fx.fusion
            .lock_nft(ctx, wrong_amount, &p2, Conditions::new()),
        Err(DriverError::UnsupportedFusionNft)
    ));

    Ok(())
}

/// An NFT sent to the p2 puzzle that isn't on either side can't be released, whether or not the
/// singleton is spent.
#[rstest]
#[case::singleton_unspent(false)]
#[case::singleton_spent(true)]
fn test_foreign_nft_is_stuck(#[case] swap: bool) -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;

    let foreign = mint_nfts(&mut fx.sim, &mut fx.ctx, &fx.user, 1, 0)?[0];
    let p2_puzzle_hash = fx.fusion.info.p2_puzzle_hash();
    let foreign = foreign.transfer(
        &mut fx.ctx,
        &StandardLayer::new(fx.user.pk),
        p2_puzzle_hash,
        Conditions::new(),
    )?;
    fx.sim
        .spend_coins(fx.ctx.take(), std::slice::from_ref(&fx.user.sk))?;

    let mut coin_spends = if swap {
        direct_swap_spends(&mut fx)?
    } else {
        Vec::new()
    };

    let _ = spend_locked_nft(
        &mut fx.ctx,
        &fx.fusion,
        &foreign,
        SETTLEMENT_PAYMENT_HASH.into(),
    )?;
    coin_spends.extend(fx.ctx.take());

    assert!(matches!(
        fx.sim
            .spend_coins(coin_spends, std::slice::from_ref(&fx.user.sk)),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed
        ))
    ));

    Ok(())
}

#[test]
fn test_swap_validation() -> Result<()> {
    let mut fx = fixture(2, 1, 0)?;
    let ctx = &mut fx.ctx;

    let settled = fx.side_b[0].child(
        SETTLEMENT_PAYMENT_HASH.into(),
        None,
        fx.side_b[0].info.metadata,
        1,
    );
    let target = fx.user.puzzle_hash;
    let unlocks: Vec<_> = fx
        .side_a
        .iter()
        .map(|nft| FusionUnlock::new(*nft, target))
        .collect();
    let nonce = Bytes32::default();

    // The NFTs can only be swapped by side
    let cases: [(FusionDirection, &[Nft], &[FusionUnlock]); 3] = [
        (FusionDirection::Defuse, &[settled], &unlocks),
        (FusionDirection::Fuse, &[settled], &unlocks[..1]),
        (FusionDirection::Fuse, &[settled, settled], &unlocks),
    ];

    for (direction, offered, unlocks) in cases {
        assert!(matches!(
            fx.fusion.swap(ctx, direction, offered, unlocks, nonce),
            Err(DriverError::FusionNftMismatch)
        ));
    }

    // The same NFT can't be used twice
    let duplicates = [unlocks[0], unlocks[0]];
    assert!(matches!(
        fx.fusion
            .swap(ctx, FusionDirection::Fuse, &[settled], &duplicates, nonce),
        Err(DriverError::FusionNftMismatch)
    ));

    // A listed launcher id must be on the side being swapped, even if the number of NFTs is right
    let foreign = mint_nfts(&mut fx.sim, ctx, &fx.user, 1, 0)?[0];
    let foreign_settled = foreign.child(
        SETTLEMENT_PAYMENT_HASH.into(),
        None,
        foreign.info.metadata,
        1,
    );
    assert!(matches!(
        fx.fusion.swap(
            ctx,
            FusionDirection::Fuse,
            &[foreign_settled],
            &unlocks,
            nonce
        ),
        Err(DriverError::FusionNftMismatch)
    ));

    // The offered NFT must be in the settlement puzzle
    assert!(matches!(
        fx.fusion
            .swap(ctx, FusionDirection::Fuse, &fx.side_b, &unlocks, nonce),
        Err(DriverError::FusionNftMismatch)
    ));

    // And the unlocked NFTs must be locked
    let mut unlocked = unlocks.clone();
    unlocked[0].nft.info.p2_puzzle_hash = target;
    assert!(matches!(
        fx.fusion
            .swap(ctx, FusionDirection::Fuse, &[settled], &unlocked, nonce),
        Err(DriverError::FusionNftMismatch)
    ));

    // The NFTs can't be sent to somewhere that they would be stuck or free for anyone to take
    for target in [
        fx.fusion.info.p2_puzzle_hash(),
        SETTLEMENT_PAYMENT_HASH.into(),
    ] {
        let unlocks: Vec<_> = fx
            .side_a
            .iter()
            .map(|nft| FusionUnlock::new(*nft, target))
            .collect();
        assert!(matches!(
            fx.fusion
                .swap(ctx, FusionDirection::Fuse, &[settled], &unlocks, nonce),
            Err(DriverError::InvalidFusionTarget)
        ));
    }

    assert!(ctx.take().is_empty());

    Ok(())
}

#[test]
fn test_take_offer_validation() -> Result<()> {
    let mut fx = fixture(1, 2, 0)?;
    let ctx = &mut fx.ctx;
    let recipient = fx.user.puzzle_hash;

    // Only part of the side is offered
    let offer = make_offer(ctx, &fx.user, &fx.side_b[..1], &fx.side_a, recipient)?;
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    // The offer doesn't ask for the other side
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &[], recipient)?;
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    // The locked NFT wasn't provided
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, recipient)?;
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &[]),
        Err(DriverError::FusionOfferMismatch | DriverError::FusionNftMismatch)
    ));

    // The offer also asks for XCH, which the fusion won't pay
    let mut offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, recipient)?;
    let mut requested_payments = offer.requested_payments().clone();
    requested_payments.xch.push(NotarizedPayment::new(
        Bytes32::default(),
        vec![Payment::new(recipient, 1, Memos::None)],
    ));
    offer = Offer::new(
        offer.spend_bundle().clone(),
        offer.offered_coins().clone(),
        requested_payments,
        offer.asset_info().clone(),
    );
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    // XCH is offered as well, which the swap would leave for anyone to take
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, recipient)?;
    let mut offered_coins = offer.offered_coins().clone();
    offered_coins.xch.push(Coin::new(
        Bytes32::default(),
        SETTLEMENT_PAYMENT_HASH.into(),
        1,
    ));
    let offer = Offer::new(
        offer.spend_bundle().clone(),
        offered_coins,
        offer.requested_payments().clone(),
        offer.asset_info().clone(),
    );
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    // The offered NFT kept its owner, so it would be locked with one
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, recipient)?;
    let mut offered_coins = offer.offered_coins().clone();
    for nft in offered_coins.nfts.values_mut() {
        nft.info.current_owner = Some(Bytes32::new([9; 32]));
    }
    let offer = Offer::new(
        offer.spend_bundle().clone(),
        offered_coins,
        offer.requested_payments().clone(),
        offer.asset_info().clone(),
    );
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    // The payment doesn't have the hint that's announced by the puzzle
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, recipient)?;
    let mut requested_payments = offer.requested_payments().clone();
    let launcher_id = fx.side_a[0].info.launcher_id;
    requested_payments.nfts[&launcher_id][0].payments[0].memos = Memos::None;
    let offer = Offer::new(
        offer.spend_bundle().clone(),
        offer.offered_coins().clone(),
        requested_payments,
        offer.asset_info().clone(),
    );
    assert!(matches!(
        fx.fusion.take_offer(ctx, &offer, &fx.side_a),
        Err(DriverError::FusionOfferMismatch)
    ));

    Ok(())
}

const DIRECT_NONCE: Bytes32 = Bytes32::new([42; 32]);

/// A fuse with no assertions in the user's spend, so that the puzzles are the only thing being
/// tested. This isn't safe outside of a test, as [`test_direct_swap_needs_assertion`] shows.
fn direct_swap_spends(fx: &mut Fixture) -> Result<Vec<CoinSpend>> {
    let ctx = &mut fx.ctx;

    let settled = fx.side_b[0].lock_settlement(
        ctx,
        &StandardLayer::new(fx.user.pk),
        Vec::new(),
        Conditions::new(),
    )?;

    fx.fusion.swap(
        ctx,
        FusionDirection::Fuse,
        &[settled],
        &[FusionUnlock::new(fx.side_a[0], fx.user.puzzle_hash)],
        DIRECT_NONCE,
    )?;

    Ok(ctx.take())
}

/// Replaces the solution of the fusion singleton, which is a way to test the puzzle itself rather
/// than the checks made by the driver. All of the other spends are left as they are.
fn tamper(
    ctx: &mut SpendContext,
    coin_spends: &mut [CoinSpend],
    fusion: &Fusion,
    tamper: impl FnOnce(&mut FusionSingletonSolution),
) -> Result<()> {
    let coin_spend = coin_spends
        .iter_mut()
        .find(|cs| cs.coin == fusion.coin)
        .expect("fusion spend");

    let solution = ctx.alloc(&coin_spend.solution)?;
    let mut solution = SingletonLayer::<FusionLayer>::parse_solution(ctx, solution)?;
    tamper(&mut solution.inner_solution);
    coin_spend.solution = ctx.serialize(&solution)?;

    Ok(())
}

fn fuse_spends(fx: &mut Fixture) -> Result<(Offer, Vec<CoinSpend>)> {
    let ctx = &mut fx.ctx;
    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, fx.user.puzzle_hash)?;
    fx.fusion.take_offer(ctx, &offer, &fx.side_a)?;
    let coin_spends = ctx.take();
    Ok((offer, coin_spends))
}

type S = FusionSingletonSolution;

#[rstest]
#[case::no_nfts_to_unlock(ErrorCode::GeneratorRuntimeError, |s: &mut S| s.nfts_to_unlock.clear())]
#[case::no_nfts_to_lock(ErrorCode::GeneratorRuntimeError, |s: &mut S| s.nfts_to_lock.clear())]
#[case::missing_nft_to_lock(ErrorCode::GeneratorRuntimeError, |s: &mut S| { s.nfts_to_lock.pop(); })]
#[case::extra_nft_to_lock(ErrorCode::GeneratorRuntimeError, |s: &mut S| s.nfts_to_lock.push(s.nfts_to_lock[0]))]
#[case::wrong_order(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_lock.reverse())]
#[case::unbalanced_direction(ErrorCode::GeneratorRuntimeError, |s: &mut S| s.unlock_a = false)]
#[case::wrong_nonce(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.offer_nonce = Bytes32::default())]
#[case::wrong_target(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_unlock[0].target_p2_puzzle_hash = ATTACKER)]
#[case::wrong_lock_metadata(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_lock[0].metadata_hash = ATTACKER)]
#[case::wrong_unlock_metadata(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_unlock[0].metadata_hash = ATTACKER)]
#[case::wrong_owner(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_lock[0].current_owner = Some(ATTACKER))]
#[case::wrong_transfer_program(ErrorCode::AssertPuzzleAnnouncementFailed, |s: &mut S| s.nfts_to_lock[0].transfer_program_hash = ATTACKER)]
#[case::wrong_unlock_coin(ErrorCode::AssertCoinAnnouncementFailed, |s: &mut S| s.nfts_to_unlock[0].coin_id = ATTACKER)]
#[case::wrong_coin_id(ErrorCode::AssertMyCoinIdFailed, |s: &mut S| s.my_id = ATTACKER)]
#[case::wrong_inner_puzzle_hash(ErrorCode::AssertMyPuzzleHashFailed, |s: &mut S| s.my_inner_puzzle_hash = ATTACKER)]
#[case::wrong_amount(ErrorCode::AssertMyAmountFailed, |s: &mut S| s.my_amount = 3)]
fn test_puzzle_rejects_tampered_solution(
    #[case] error: ErrorCode,
    #[case] tamper_solution: impl FnOnce(&mut FusionSingletonSolution),
) -> Result<()> {
    let mut fx = fixture(1, 2, 0)?;
    let (offer, mut coin_spends) = fuse_spends(&mut fx)?;

    tamper(&mut fx.ctx, &mut coin_spends, &fx.fusion, tamper_solution)?;

    assert!(matches!(
        take(&mut fx.sim, offer, coin_spends),
        Err(SimulatorError::Validation(code)) if code == error
    ));

    Ok(())
}

/// The solution isn't signed, so anyone can make these changes to a spend without affecting what it
/// does, and the child must still be found.
#[rstest]
#[case::unlock_a_two(Bytes::from(vec![2]), false)]
#[case::unlock_a_zero_byte(Bytes::from(vec![0]), false)]
#[case::unlock_a_long(Bytes::from(vec![1, 2, 3, 4]), false)]
#[case::nil_lock_coin_id(Bytes::from(vec![1]), true)]
fn test_parse_child_of_altered_solution(
    #[case] unlock_a: Bytes,
    #[case] nil_lock_coin_id: bool,
) -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, fx.user.puzzle_hash)?;
    let swap = fx.fusion.take_offer(ctx, &offer, &fx.side_a)?;
    let mut coin_spends = ctx.take();

    let coin_spend = coin_spends
        .iter_mut()
        .find(|cs| cs.coin == fx.fusion.coin)
        .expect("fusion spend");
    let solution = ctx.alloc(&coin_spend.solution)?;
    let parsed = SingletonLayer::<FusionLayer>::parse_solution(ctx, solution)?;
    let inner = parsed.inner_solution;

    let lock = inner.nfts_to_lock[0];
    let nfts_to_lock = if nil_lock_coin_id {
        ctx.alloc(&vec![clvm_list!(
            (),
            lock.metadata_hash,
            lock.current_owner,
            lock.transfer_program_hash
        )])?
    } else {
        ctx.alloc(&inner.nfts_to_lock)?
    };

    let inner_solution = ctx.alloc(&clvm_list!(
        inner.my_id,
        inner.my_inner_puzzle_hash,
        inner.my_amount,
        nfts_to_lock,
        inner.nfts_to_unlock.clone(),
        unlock_a,
        inner.offer_nonce
    ))?;
    coin_spend.solution = ctx.serialize(&clvm_list!(
        parsed.lineage_proof,
        parsed.amount,
        inner_solution,
        "trailing data"
    ))?;

    take(&mut fx.sim, offer, coin_spends)?;
    assert_swap(
        &fx.sim,
        &swap,
        fx.user.puzzle_hash,
        fx.fusion.info.p2_puzzle_hash(),
    );

    let spend = fx.sim.coin_spend(fx.fusion.coin.coin_id()).expect("spend");
    let puzzle = ctx.alloc(&spend.puzzle_reveal)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    let solution = ctx.alloc(&spend.solution)?;
    assert_eq!(
        Fusion::parse_child(ctx, spend.coin, puzzle, solution)?,
        Some((swap.fusion.clone(), FusionDirection::Fuse))
    );

    Ok(())
}

#[test]
fn test_parse_child_rejects_truncated_solution() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let puzzle = SingletonLayer::new(fx.fusion.info.launcher_id, fx.fusion.info.clone())
        .construct_puzzle(ctx)?;
    let puzzle = Puzzle::parse(ctx, puzzle);
    let solution = ctx.alloc(&clvm_list!(1, 1, clvm_list!(1, 2, 3)))?;

    assert!(matches!(
        Fusion::parse_child(ctx, fx.fusion.coin, puzzle, solution),
        Err(DriverError::InvalidFusionSolution)
    ));

    Ok(())
}

/// A launcher can create a singleton with any amount, but it needs to be odd to ever be spent.
#[test]
fn test_even_singleton_amount() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(0);
    let user = sim.bls(0);
    let side_a = mint_nfts(&mut sim, ctx, &house, 1, 0)?;
    let side_b = mint_nfts(&mut sim, ctx, &user, 1, 0)?;
    let nft_a = side_a[0].info.launcher_id;
    let nft_b = side_b[0].info.launcher_id;

    // The driver won't launch one
    let coin = sim.new_coin(house.puzzle_hash, 2);
    let launcher = Launcher::for_fusion(ctx, coin.coin_id())?.with_singleton_amount(2);
    assert!(matches!(
        launcher.launch_fusion(ctx, vec![nft_a], vec![nft_b]),
        Err(DriverError::InvalidFusionSingletonAmount(2))
    ));

    // But an attacker can launch one with the real puzzle, which is a fusion that's invalid
    let launcher = Launcher::for_fusion(ctx, coin.coin_id())?.with_singleton_amount(2);
    let launcher_coin = launcher.coin();
    let info = FusionLayer::new(launcher_coin.coin_id(), vec![nft_a], vec![nft_b])?;
    let memo = FusionMemo {
        launcher_id: launcher_coin.coin_id(),
        side_a: vec![nft_a],
        side_b: vec![nft_b],
    };
    let (conditions, _) = launcher.spend(ctx, info.inner_puzzle_hash().into(), memo)?;
    StandardLayer::new(house.pk).spend(ctx, coin, conditions)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    let launcher_spend = sim.coin_spend(launcher_coin.coin_id()).expect("launcher");
    let solution = ctx.alloc(&launcher_spend.solution)?;
    assert!(matches!(
        Fusion::parse_launcher(ctx, launcher_spend.coin, solution),
        Err(DriverError::InvalidFusionSingletonAmount(2))
    ));

    Ok(())
}

/// The sides are invalid, but the launcher did launch them, so this isn't reported as no fusion.
#[test]
fn test_parse_launcher_invalid_sides() -> Result<()> {
    let ctx = &mut SpendContext::new();

    let launcher_coin = Coin::new(Bytes32::default(), SINGLETON_LAUNCHER_HASH.into(), 1);
    let launcher_id = launcher_coin.coin_id();
    let puzzle_hash: Bytes32 = SingletonArgs::curry_tree_hash(
        launcher_id,
        FusionSingletonArgs::new(launcher_id, vec![], vec![]).curry_tree_hash(),
    )
    .into();

    let empty: Vec<Bytes32> = Vec::new();
    let solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        clvm_list!(launcher_id, empty.clone(), empty)
    ))?;

    assert!(matches!(
        Fusion::parse_launcher(ctx, launcher_coin, solution),
        Err(DriverError::InvalidFusion)
    ));

    Ok(())
}

/// With the same number of NFTs on each side, the direction isn't caught by the list lengths.
#[test]
fn test_puzzle_rejects_wrong_direction() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let (offer, mut coin_spends) = fuse_spends(&mut fx)?;

    tamper(&mut fx.ctx, &mut coin_spends, &fx.fusion, |solution| {
        solution.unlock_a = false;
    })?;

    assert!(matches!(
        take(&mut fx.sim, offer, coin_spends),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed | ErrorCode::AssertCoinAnnouncementFailed
        ))
    ));

    Ok(())
}

/// The p2 puzzle only releases the NFT to the settlement puzzle hash that's announced by the
/// singleton, so nobody can send it straight to themselves.
#[test]
fn test_locked_nft_cannot_be_stolen() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let _ = spend_locked_nft(ctx, &fx.fusion, &fx.side_a[0], ATTACKER)?;

    assert!(matches!(
        fx.sim.spend_coins(ctx.take(), &[]),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed
        ))
    ));

    Ok(())
}

/// Even alongside a swap, where the only thing stopping it is the singleton and p2 puzzles.
#[test]
fn test_locked_nft_cannot_be_stolen_during_swap() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let mut coin_spends = direct_swap_spends(&mut fx)?;

    let locked = fx.side_a[0];
    coin_spends
        .retain(|cs| cs.coin != locked.coin && cs.coin.parent_coin_info != locked.coin.coin_id());

    let _ = spend_locked_nft(&mut fx.ctx, &fx.fusion, &locked, ATTACKER)?;
    coin_spends.extend(fx.ctx.take());

    assert!(matches!(
        fx.sim
            .spend_coins(coin_spends, std::slice::from_ref(&fx.user.sk)),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed | ErrorCode::AssertCoinAnnouncementFailed
        ))
    ));

    Ok(())
}

/// The singleton requires the unlocked NFT to pay the target, even if nothing else asserts it.
#[test]
fn test_unlocked_nft_cannot_be_redirected() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let mut coin_spends = direct_swap_spends(&mut fx)?;
    let ctx = &mut fx.ctx;

    let settlement_nft = fx.side_a[0].child(
        SETTLEMENT_PAYMENT_HASH.into(),
        None,
        fx.side_a[0].info.metadata,
        1,
    );

    coin_spends.retain(|cs| cs.coin != settlement_nft.coin);
    let payment = settlement_payment(ctx, DIRECT_NONCE, ATTACKER)?;
    let _ = settlement_nft.unlock_settlement(ctx, vec![payment])?;
    coin_spends.extend(ctx.take());

    assert!(matches!(
        fx.sim
            .spend_coins(coin_spends, std::slice::from_ref(&fx.user.sk)),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed
        ))
    ));

    Ok(())
}

/// The singleton requires the offered NFT to pay the p2 puzzle, so it can't be kept by the taker.
#[test]
fn test_offered_nft_cannot_be_redirected() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let mut coin_spends = direct_swap_spends(&mut fx)?;
    let ctx = &mut fx.ctx;

    let settlement_nft = fx.side_b[0].child(
        SETTLEMENT_PAYMENT_HASH.into(),
        None,
        fx.side_b[0].info.metadata,
        1,
    );

    coin_spends.retain(|cs| cs.coin != settlement_nft.coin);
    let payment = settlement_payment(ctx, DIRECT_NONCE, ATTACKER)?;
    let _ = settlement_nft.unlock_settlement(ctx, vec![payment])?;
    coin_spends.extend(ctx.take());

    assert!(matches!(
        fx.sim
            .spend_coins(coin_spends, std::slice::from_ref(&fx.user.sk)),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed
        ))
    ));

    Ok(())
}

/// A real NFT in the settlement puzzle can't stand in for the one on the fusion's side, since the
/// puzzle hash that's asserted is derived from the curried launcher id.
#[test]
fn test_unlisted_nft_cannot_be_offered() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;

    let foreign = mint_nfts(&mut fx.sim, &mut fx.ctx, &fx.user, 1, 0)?[0];
    let mut coin_spends = direct_swap_spends(&mut fx)?;
    let ctx = &mut fx.ctx;

    // The user's NFT isn't spent, and the foreign one pays the p2 puzzle in its place
    let listed = fx.side_b[0];
    coin_spends
        .retain(|cs| cs.coin != listed.coin && cs.coin.parent_coin_info != listed.coin.coin_id());

    let settled = foreign.lock_settlement(
        ctx,
        &StandardLayer::new(fx.user.pk),
        Vec::new(),
        Conditions::new(),
    )?;
    let payment = settlement_payment(ctx, DIRECT_NONCE, fx.fusion.info.p2_puzzle_hash())?;
    let _ = settled.unlock_settlement(ctx, vec![payment])?;
    coin_spends.extend(ctx.take());

    assert!(matches!(
        fx.sim
            .spend_coins(coin_spends, std::slice::from_ref(&fx.user.sk)),
        Err(SimulatorError::Validation(
            ErrorCode::AssertPuzzleAnnouncementFailed
        ))
    ));

    Ok(())
}

/// Offers can't be taken before the NFTs are locked, since the singleton would have nothing to pay.
#[test]
fn test_swap_with_nothing_locked_fails() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(0);
    let user = sim.bls(0);
    let side_a = mint_nfts(&mut sim, ctx, &house, 1, 0)?;
    let side_b = mint_nfts(&mut sim, ctx, &user, 1, 0)?;
    let fusion = launch(&mut sim, ctx, &house, &side_a, &side_b)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&house.sk))?;

    let offer = make_offer(ctx, &user, &side_b, &side_a, user.puzzle_hash)?;

    assert!(matches!(
        fusion.take_offer(ctx, &offer, &side_a),
        Err(DriverError::FusionNftMismatch)
    ));

    Ok(())
}

/// The singleton asserts a relative height, so a swap can't be influenced by an ephemeral singleton.
#[test]
fn test_ephemeral_singleton_spend_fails() -> Result<()> {
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();

    let house = sim.bls(0);
    let user = sim.bls(0);
    let side_a = mint_nfts(&mut sim, ctx, &house, 1, 0)?;
    let side_b = mint_nfts(&mut sim, ctx, &user, 1, 0)?;

    // The fusion is launched and side A is locked in the same transaction as the swap
    let fusion = launch(&mut sim, ctx, &house, &side_a, &side_b)?;
    let p2 = StandardLayer::new(house.pk);
    let locked = fusion.lock_nft(ctx, side_a[0], &p2, Conditions::new())?;
    let house_spends = ctx.take();

    let nonce = Bytes32::default();
    let assertion = assert_payment(ctx, &locked, nonce, user.puzzle_hash)?;
    let settled = side_b[0].lock_settlement(
        ctx,
        &StandardLayer::new(user.pk),
        Vec::new(),
        Conditions::new().with(assertion),
    )?;
    fusion.swap(
        ctx,
        FusionDirection::Fuse,
        &[settled],
        &[FusionUnlock::new(locked, user.puzzle_hash)],
        nonce,
    )?;

    let coin_spends = [house_spends, ctx.take()].concat();

    assert!(matches!(
        sim.spend_coins(coin_spends, &[house.sk, user.sk]),
        Err(SimulatorError::Validation(
            ErrorCode::EphemeralRelativeCondition
        ))
    ));

    Ok(())
}

/// Each swap needs the current fusion coin, so a wallet that's behind can't make one.
#[test]
fn test_stale_fusion_coin_fails() -> Result<()> {
    let mut fx = fixture(1, 1, 0)?;
    let ctx = &mut fx.ctx;

    let offer = make_offer(ctx, &fx.user, &fx.side_b, &fx.side_a, fx.user.puzzle_hash)?;
    let swap = fx.fusion.take_offer(ctx, &offer, &fx.side_a)?;
    take(&mut fx.sim, offer, ctx.take())?;

    let offer = make_offer(
        ctx,
        &fx.user,
        &swap.unlocked,
        &swap.locked,
        fx.user.puzzle_hash,
    )?;
    fx.fusion.take_offer(ctx, &offer, &swap.locked)?;

    assert!(matches!(
        take(&mut fx.sim, offer, ctx.take()),
        Err(SimulatorError::Validation(ErrorCode::DoubleSpend))
    ));

    Ok(())
}

fn spend_locked_nft(
    ctx: &mut SpendContext,
    fusion: &Fusion,
    nft: &Nft,
    next_puzzle_hash: Bytes32,
) -> Result<Nft> {
    let spend = P2FusionLayer::new(fusion.info.launcher_id).spend(
        ctx,
        P2FusionSolution {
            singleton_inner_puzzle_hash: fusion.info.inner_puzzle_hash().into(),
            singleton_coin_id: fusion.coin.coin_id(),
            nft_launcher_id: nft.info.launcher_id,
            nft_inner_puzzle_hash: nft.info.inner_puzzle_hash().into(),
            my_amount: nft.coin.amount,
            next_puzzle_hash,
        },
    )?;
    Ok(nft.spend(ctx, spend)?)
}

/// The solution of the first swap of a fusion that was deployed on mainnet, which fused NFT B into
/// NFT A, parsed and run against the puzzles and types from this crate.
#[test]
fn test_mainnet_swap() -> Result<()> {
    let ctx = &mut SpendContext::new();

    let launcher_coin = Coin::new(
        hex!("07a013b49e4947408b879a51bd8e7569754e02190102fcaed11c343743b808f5").into(),
        SINGLETON_LAUNCHER_HASH.into(),
        1,
    );
    let nft_a: Bytes32 =
        hex!("6f8603f7521fda4f62148b96ec2a0ef63ddb0943f36ef08c4ae598339c1c2353").into();
    let nft_b: Bytes32 =
        hex!("fcd4b808ad2e3d6c07b512552b7a8facd12b69cfb64979b5c776e91aec89fea3").into();
    let puzzle_hash: Bytes32 =
        hex!("c4b70a7b6f397952f7a575bfcea6f3cc2e890900965e2de34c8db803d31636ea").into();

    let launcher_id = launcher_coin.coin_id();
    assert_eq!(
        launcher_id,
        Bytes32::new(hex!(
            "29e6c03fe0fbcca3d4edf2c7e703c4d0fb2a3cdfe157f57c0dfd3a67abeb4d36"
        ))
    );

    // The launcher solution as it was on mainnet, with the data in the key value list
    let launcher_solution = ctx.alloc(&clvm_list!(
        puzzle_hash,
        1,
        clvm_list!(launcher_id, vec![nft_a], vec![nft_b])
    ))?;
    let fusion =
        Fusion::parse_launcher(ctx, launcher_coin, launcher_solution)?.expect("fusion launcher");
    assert_eq!(fusion.coin.puzzle_hash, puzzle_hash);

    let solution = Program::from(
        hex!(
            "
        ffffa007a013b49e4947408b879a51bd8e7569754e02190102fcaed11c343743
        b808f5ff0180ff01ffffa0f3febb93a7b4f1af2892365f59f3b73ddb21ad842e
        e0ddeb6b2287268e2b638effa08fb51fa546975dd05fe4eb88a6423b73054207
        751dbfbe3c742c6652860077b3ff01ffffffa04596f695e499363de86ce9f2f8
        4c48484666f98b54675d90a45b852f1c305c3dffa0608a456aee26912718f636
        a03984ba593734a0ffa3fee109cf8cf6c666e83888ff80ffa066aa7595ce1555
        8333d1bbdd8bf57d85a2af2d5d294d4454e6ce4342528d51fa8080ffffffa098
        d4699b97228733cc9123b676dc795b8740d73a4c6d649a6e0bcfc49eece973ff
        a04b43bee462edb17ea5ccdad37af20ba463db838fb2cf5e26dac1c5398fe833
        d3ff80ffa0f609fd2c508be04fdae25e681b506108e14a5b34c673717597d7c0
        72d0eadcb9ffa0180a8941695bad54a162d8c15298d1ba45ddac0bc2633e0da0
        0d8d0ff10a4ac38080ff01ffa00d91c120598b981b6bd1dfefea094175eb55c8
        c725f7f29101829b5134036cf38080
        "
        )
        .to_vec(),
    );

    // The solution parses as a fuse, and is serialized the same way
    let solution_ptr = ctx.alloc(&solution)?;
    let parsed = SingletonLayer::<FusionLayer>::parse_solution(ctx, solution_ptr)?;
    assert_eq!(parsed.lineage_proof, fusion.proof);
    assert!(parsed.inner_solution.unlock_a);
    assert_eq!(ctx.serialize(&parsed)?, solution);

    // The child is hinted, and the singleton asserts the release of the locked NFT
    let puzzle = SingletonLayer::new(launcher_id, fusion.info.clone()).construct_puzzle(ctx)?;
    assert_eq!(ctx.tree_hash(puzzle), TreeHash::from(puzzle_hash));
    let output = ctx.run(puzzle, solution_ptr)?;
    let conditions: Vec<Condition> = ctx.extract(output)?;

    let create_coin = conditions
        .iter()
        .find_map(|condition| condition.clone().into_create_coin())
        .expect("singleton child");
    assert_eq!(create_coin.puzzle_hash, puzzle_hash);
    let Memos::Some(memos) = create_coin.memos else {
        panic!("missing memos");
    };
    assert_eq!(
        ctx.extract::<Vec<Bytes32>>(memos)?,
        vec![FUSION_HINT, launcher_id]
    );

    let nft_coin = Coin::new(
        hex!("ecfc42d75b10da6ea7073b5f470b2979b52ef2f8ec85d4a6fc66925729cefe93").into(),
        hex!("a0057a77e1babe4d5d5dff5879aada2b5b4ce390cd401fe0c37578fa2422a052").into(),
        1,
    );
    assert_eq!(
        nft_coin.coin_id(),
        parsed.inner_solution.nfts_to_unlock[0].coin_id
    );

    // The locked NFT's p2 solution from the same spend bundle, which goes to the settlement puzzle
    let nft_solution = Program::from(
        hex!(
            "
        ffffa06f8603f7521fda4f62148b96ec2a0ef63ddb0943f36ef08c4ae598339c
        1c2353ffa0e4c237609c801c85ad5d494d3b8abf51e55c471710a071360302e8
        fe55b202baff0180ff01ffffffffa08fb51fa546975dd05fe4eb88a6423b7305
        4207751dbfbe3c742c6652860077b3ffa0f3febb93a7b4f1af2892365f59f3b7
        3ddb21ad842ee0ddeb6b2287268e2b638effa06f8603f7521fda4f62148b96ec
        2a0ef63ddb0943f36ef08c4ae598339c1c2353ffa015c5bff635de8352579230
        bf5f54a74eb4794d915b20f41545c6554053abdce1ff01ffa0cfbfdeed5c4ca2
        de3d0bf520b9cb4bb7743a359bd2e6a188d19ce7dffc21d3e780808080
        "
        )
        .to_vec(),
    );
    let nft_solution_ptr = ctx.alloc(&nft_solution)?;
    let nft_solution: SingletonSolution<
        NftStateLayerSolution<NftOwnershipLayerSolution<P2FusionSolution>>,
    > = ctx.extract(nft_solution_ptr)?;
    let p2_solution = nft_solution.inner_solution.inner_solution.inner_solution;
    assert_eq!(p2_solution.next_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    let p2 = P2FusionLayer::new(launcher_id);
    let p2_puzzle = p2.construct_puzzle(ctx)?;
    let p2_solution_ptr = ctx.alloc(&p2_solution)?;
    let p2_output = ctx.run(p2_puzzle, p2_solution_ptr)?;
    let p2_conditions: Vec<Condition> = ctx.extract(p2_output)?;

    // Each side makes the announcement asserted by the other
    let message = p2_conditions
        .iter()
        .find_map(|condition| condition.clone().into_create_coin_announcement())
        .expect("p2 announcement")
        .message;
    let expected = announcement_id(nft_coin.coin_id(), message.clone());
    assert!(conditions.iter().any(|condition| {
        condition
            .clone()
            .into_assert_coin_announcement()
            .is_some_and(|assertion| assertion.announcement_id == expected)
    }));

    let expected = announcement_id(puzzle_hash, message);
    assert!(p2_conditions.iter().any(|condition| {
        condition
            .clone()
            .into_assert_puzzle_announcement()
            .is_some_and(|assertion| assertion.announcement_id == expected)
    }));

    Ok(())
}
