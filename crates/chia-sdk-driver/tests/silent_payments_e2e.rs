#![cfg(feature = "chip-0057")]

//! End-to-end simulator tests for CHIP-0057 silent payments.
//!
//! These tests run the full send -> farm -> extract -> scan -> detect -> spend
//! flow against [`chia_sdk_test::Simulator`], exercising the workspace's
//! receive primitive, send-side action plumbing, and the canonical
//! [`chia_sdk_test::silent_payments::tweak_data_from_simulator_block`] helper
//! together.
//!
//! Living in the integration-test target lets these tests call the canonical
//! cross-crate helper directly. Inside the lib's `#[cfg(test)]` module the same
//! call would surface as type confusion because the build graph would carry
//! two structurally distinct copies of
//! `chia_sdk_driver::silent_payments::TweakData` (the
//! `chia-sdk-driver -> chia-sdk-test -> chia-sdk-driver` dev-dep cycle). The
//! integration target builds against the lib's published types only, so the
//! cycle resolves to a single type identity.

use anyhow::Result;
use bip39::Mnemonic;
use chia_protocol::SpendBundle;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::offer::{NotarizedPayment, Payment, SettlementPaymentsSolution};
use chia_puzzle_types::standard::StandardArgs;
use chia_puzzle_types::{DeriveSynthetic, Memos};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_driver::silent_payments::{
    K_MAX_DEFAULT, OtherPayment, OutputMeta, SettlementGroupCoin, SettlementRecipient,
    SyntheticPublicKey, SyntheticSecretKey, Wrapper, aggregate_sender_sks, compute_input_hash,
    compute_tweak_point, derive_one_time_puzzle_hash, derive_settlement_outputs, scan_from_tweaks,
    settlement_announcement, settlement_puzzle_hash, spend_groups_from_block_spends,
    tweak_data_from_block_spends,
};
use chia_sdk_driver::{
    Action, AssetInfo, Cat, CatAssetInfo, CatSpend, DriverError, FungibleSpend, HashedPtr, Id,
    Offer, Relation, RequestedPayments, SilentPaymentSendAction, SpendContext, SpendWithConditions,
    Spends, StandardLayer,
};
use chia_sdk_test::silent_payments::tweak_data_from_simulator_block;
use chia_sdk_test::{BlsPairWithCoin, Simulator, sign_transaction};
use chia_sdk_types::silent_payments::GROUP_ORDER;
use chia_sdk_types::{Condition, Conditions, run_puzzle, tree_hash_notarized_payment};
use chia_sdk_utils::silent_payments::{LabelRegistry, SilentPaymentKeys, SilentPaymentNetwork};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::Allocator;
use indexmap::indexmap;

/// The BIP-39 test mnemonic — matches the cross-language AVA fixture for
/// deterministic seeds. The recipient keys are derived from it with hardened
/// derivation (CHIP-0057 Test Vector 8).
const TEST_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

/// Shared setup: fresh [`Simulator`] + [`SpendContext`] + sender BLS pair (with
/// a `1_000`-mojo XCH coin) + a recipient [`SilentPaymentKeys`] derived from a
/// deterministic mnemonic.
///
/// Each test calls this fresh — no shared state, deterministic seeds, every
/// run produces identical `coin_id`s.
fn setup_e2e() -> Result<(Simulator, SpendContext, BlsPairWithCoin, SilentPaymentKeys)> {
    let mut sim = Simulator::new();
    let ctx = SpendContext::new();
    let sender = sim.bls(1_000);
    let mnemonic = Mnemonic::parse(TEST_MNEMONIC)?;
    let recipient = SilentPaymentKeys::from_mnemonic(&mnemonic);
    Ok((sim, ctx, sender, recipient))
}

/// Unlabeled SP send round-trip against the simulator.
///
/// Asserts:
/// 1. Exactly one detection.
/// 2. `label: None` (unlabeled).
/// 3. `k == 0` (first output to this `scan_pk` in the tx).
/// 4. `amount == 100`.
/// 5. The detected coin spends successfully via [`StandardLayer`] after
///    deriving the one-time key from the detection with the spend secret key
///    and applying [`DeriveSynthetic::derive_synthetic`] to it.
#[test]
fn test_simulator_e2e_unlabeled() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let recipient_address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    // Build the SP send via Action::silent_payment_send.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            recipient_address,
            100,
            Memos::None,
        )],
    )?;
    // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap the
    // raw `sim.bls()` fixture key via `from_synthetic_unchecked` (the coin is
    // curried over the raw pk, so the registered key IS the raw key).
    let pk_map = indexmap! { sender.puzzle_hash => sender.pk };
    let synthetic_public_map = indexmap! {
        sender.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender.pk),
    };
    let synthetic_secret_map = indexmap! {
        sender.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender.sk.clone()),
    };
    spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;

    // Farm: spend_coins farms a block internally.
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;

    // Extract: build TweakData from the freshly-farmed block via the canonical
    // chia-sdk-test helper.
    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    assert!(
        !tweak_data.tweak_points.is_empty(),
        "expected one tweak_point"
    );
    assert!(
        !tweak_data.outputs.is_empty(),
        "expected at least the recipient's output"
    );

    // Scan: recipient detects the coin via `scan_from_tweaks` (free-fn form).
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1, "expected exactly 1 detection");
    let detected = &detections[0];
    assert!(detected.label.is_none(), "unlabeled -> label must be None");
    assert_eq!(detected.k, 0, "first output -> k=0");
    assert_eq!(detected.amount, 100);

    // Spend the detected coin. Scanning used only the scan secret key and the
    // spend public key; the spend secret key is first needed here.
    let synthetic_secret = detected.onetime_sk(recipient.spend_sk()).derive_synthetic();
    let conditions = Conditions::new()
        .create_coin(sender.puzzle_hash, detected.amount - 1, Memos::None)
        .reserve_fee(1);
    let coin = Coin::new(
        detected.parent_coin_id,
        detected.puzzle_hash,
        detected.amount,
    );
    StandardLayer::new(synthetic_secret.public_key()).spend(&mut ctx, coin, conditions)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;

    // Verify: the detected coin is now spent.
    let post_state = sim
        .coin_state(detected.coin_id)
        .expect("detected coin in state");
    assert!(
        post_state.spent_height.is_some(),
        "detected coin must be spent after follow-on spend"
    );

    Ok(())
}

/// Multi-input unlabeled SP send round-trip: two XCH inputs with different
/// keys sent to one unlabeled SP address with `Relation::AssertConcurrent`.
///
/// This drives the real receiver grouping path: the sender aggregates both
/// inputs' synthetic SKs, the `AssertConcurrent` cycle binds the two coins into
/// one strongly connected component, and the receiver reconstructs the same
/// `input_hash` over the same input set.
///
/// Asserts:
/// 1. The `AssertConcurrent` runtime gate is satisfied (2 spent XCH
///    inputs + `Relation::AssertConcurrent`) — `finish_with_keys` does NOT error
///    with `SilentPaymentRequiresInputBinding`.
/// 2. Exactly one detection at the expected one-time puzzle hash.
/// 3. `label: None`, `k == 0`, `amount == 1000`.
/// 4. The detected coin spends successfully.
#[test]
fn test_simulator_e2e_multi_input() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    // Two distinct sender key pairs / puzzle hashes / coins.
    let a = sim.bls(600);
    let b = sim.bls(600);

    let mnemonic = Mnemonic::parse(TEST_MNEMONIC)?;
    let recipient = SilentPaymentKeys::from_mnemonic(&mnemonic);
    let recipient_address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    // Both coins as inputs; change returns to `a.puzzle_hash`.
    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(b.coin);

    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            recipient_address,
            1000,
            Memos::None,
        )],
    )?;

    // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap each
    // raw `sim.bls()` fixture key via `from_synthetic_unchecked` (coins curried
    // over the raw pk, so the registered key IS the raw key). Both puzzle hashes
    // must be present in every map.
    let pk_map = indexmap! {
        a.puzzle_hash => a.pk,
        b.puzzle_hash => b.pk,
    };
    let synthetic_public_map = indexmap! {
        a.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(a.pk),
        b.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(b.pk),
    };
    let synthetic_secret_map = indexmap! {
        a.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(a.sk.clone()),
        b.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(b.sk.clone()),
    };
    spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

    // MUST be AssertConcurrent when 2+ XCH coins are spent — otherwise the
    // runtime gate errors with SilentPaymentRequiresInputBinding.
    spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;

    // Farm: sign with BOTH senders' SKs.
    sim.spend_coins(ctx.take(), &[a.sk.clone(), b.sk.clone()])?;

    // Extract + scan via the canonical helper + free-fn scanner.
    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );

    assert_eq!(
        detections.len(),
        1,
        "multi-input send must produce exactly one detection"
    );
    let detected = &detections[0];
    assert!(detected.label.is_none(), "unlabeled -> label must be None");
    assert_eq!(detected.k, 0, "first output -> k=0");
    assert_eq!(detected.amount, 1000);

    // Spend the detected coin.
    let synthetic_secret = detected.onetime_sk(recipient.spend_sk()).derive_synthetic();
    let conditions = Conditions::new()
        .create_coin(a.puzzle_hash, detected.amount - 1, Memos::None)
        .reserve_fee(1);
    let coin = Coin::new(
        detected.parent_coin_id,
        detected.puzzle_hash,
        detected.amount,
    );
    StandardLayer::new(synthetic_secret.public_key()).spend(&mut ctx, coin, conditions)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;

    let post_state = sim
        .coin_state(detected.coin_id)
        .expect("detected coin in state");
    assert!(
        post_state.spent_height.is_some(),
        "detected coin must be spent after follow-on spend"
    );

    Ok(())
}

/// CHIP-0057 closed list of eligible spends: a DID or an NFT is not an
/// eligible spend, and the `ASSERT_CONCURRENT_SPEND` cycle would pass through
/// it, so a silent payment send whose `Spends` also creates (or spends) a DID
/// or an NFT is rejected with `SilentPaymentIneligibleAssetBundle` before any
/// derivation or key work, and no coin spend is emitted.
#[test]
fn test_did_or_nft_in_the_bundle_is_rejected() -> Result<()> {
    let mut sim = Simulator::new();
    let a = sim.bls(600);
    let b = sim.bls(600);
    let recipient = SilentPaymentKeys::from_mnemonic(&Mnemonic::parse(TEST_MNEMONIC)?);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);

    let singleton_actions = [
        Action::create_did(None, 1, HashedPtr::NIL, 1),
        Action::mint_nft(HashedPtr::NIL, Bytes32::default(), a.puzzle_hash, 0, 1),
    ];
    for singleton_action in singleton_actions {
        let mut ctx = SpendContext::new();
        let mut spends = Spends::new(a.puzzle_hash);
        spends.add(a.coin);
        spends.add(b.coin);
        let deltas = spends.apply(
            &mut ctx,
            &[
                singleton_action.clone(),
                Action::silent_payment_send(address.clone(), 100, Memos::None),
            ],
        )?;
        let pk_map = register_keys(&mut spends, &[&a, &b]);
        let result =
            spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
        assert!(
            matches!(result, Err(DriverError::SilentPaymentIneligibleAssetBundle)),
            "{singleton_action:?} with a silent payment must be rejected, got {result:?}"
        );
        // Only the singleton's launcher, spent when the action was applied, is in
        // the context: no XCH coin is spent, so no silent payment is created.
        let emitted = ctx.take();
        assert!(
            emitted
                .iter()
                .all(|cs| cs.coin != a.coin && cs.coin != b.coin),
            "no coin of the group is spent"
        );
    }
    Ok(())
}

/// Labeled SP send round-trip: same flow as
/// [`test_simulator_e2e_unlabeled`] using a labeled recipient address (`m=1`).
/// Asserts the scanner correctly detects with `label: Some(1)` and the labeled
/// coin spends successfully.
#[test]
fn test_simulator_e2e_labeled() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let recipient_address = recipient.labeled_address(SilentPaymentNetwork::Testnet, 1)?;
    let height_before = sim.height();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            recipient_address,
            200,
            Memos::None,
        )],
    )?;
    // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap the
    // raw `sim.bls()` fixture key via `from_synthetic_unchecked` (the coin is
    // curried over the raw pk, so the registered key IS the raw key).
    let pk_map = indexmap! { sender.puzzle_hash => sender.pk };
    let synthetic_public_map = indexmap! {
        sender.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender.pk),
    };
    let synthetic_secret_map = indexmap! {
        sender.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender.sk.clone()),
    };
    spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;

    // Register m=1 in the recipient's LabelRegistry so the scanner can detect it.
    let mut labels = LabelRegistry::new();
    labels.register(recipient.scan_sk(), 1);

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        Some(&labels),
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1, "expected exactly 1 labeled detection");
    let detected = &detections[0];
    assert_eq!(detected.label, Some(1), "labeled at m=1");
    assert_eq!(detected.amount, 200);

    // Follow-on spend (labeled): the detection's tweak already includes the
    // label scalar, so the one-time key is derived exactly as in the unlabeled
    // case.
    let synthetic_secret = detected.onetime_sk(recipient.spend_sk()).derive_synthetic();
    let conditions = Conditions::new()
        .create_coin(sender.puzzle_hash, detected.amount - 1, Memos::None)
        .reserve_fee(1);
    let coin = Coin::new(
        detected.parent_coin_id,
        detected.puzzle_hash,
        detected.amount,
    );
    StandardLayer::new(synthetic_secret.public_key()).spend(&mut ctx, coin, conditions)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;

    let post_state = sim
        .coin_state(detected.coin_id)
        .expect("detected coin in state");
    assert!(
        post_state.spent_height.is_some(),
        "labeled detected coin must be spent after follow-on spend"
    );

    Ok(())
}

/// A payment to the wallet's own UNLABELED address is reported as unlabeled
/// (`label: None`) even though the scanner always checks the change label and
/// the caller has registered it as well: the unlabeled candidate is checked
/// first, so the change label cannot hijack ordinary incoming payments.
#[test]
fn test_simulator_e2e_m0_self_change() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    // The sender's XCH coin comes from `sim.bls()` — a different key pair —
    // but the recipient SP address is the one we treat as "self" for the
    // test. The `m=0` contract being asserted is about how the recipient's
    // own LabelRegistry interacts with detection of its own unlabeled inbound
    // payments; the XCH source identity is incidental.
    let recipient_address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            recipient_address,
            300,
            Memos::None,
        )],
    )?;
    // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap the
    // raw `sim.bls()` fixture key via `from_synthetic_unchecked` (the coin is
    // curried over the raw pk, so the registered key IS the raw key).
    let pk_map = indexmap! { sender.puzzle_hash => sender.pk };
    let synthetic_public_map = indexmap! {
        sender.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender.pk),
    };
    let synthetic_secret_map = indexmap! {
        sender.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender.sk.clone()),
    };
    spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;

    // Registering m=0 is allowed and redundant; it must not change how the
    // unlabeled payment below is reported.
    let mut labels = LabelRegistry::new();
    labels.register(recipient.scan_sk(), 0);

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        Some(&labels),
        K_MAX_DEFAULT,
    );

    assert_eq!(detections.len(), 1, "self-send should detect exactly once");
    let detected = &detections[0];
    assert_eq!(
        detected.label, None,
        "m=0 in registry must NOT promote unlabeled self-send to label: Some(0)"
    );
    assert_eq!(detected.amount, 300);

    Ok(())
}

/// Registers the raw `sim.bls()` keys of the senders as the silent payment keys
/// (the fixture coins are curried over the raw key) and returns the public key
/// map for `finish_with_keys`.
fn register_keys(
    spends: &mut Spends,
    senders: &[&BlsPairWithCoin],
) -> indexmap::IndexMap<chia_protocol::Bytes32, chia_bls::PublicKey> {
    spends.with_silent_payment_keys(
        senders
            .iter()
            .map(|s| {
                (
                    s.puzzle_hash,
                    SyntheticPublicKey::from_synthetic_unchecked(s.pk),
                )
            })
            .collect(),
        senders
            .iter()
            .map(|s| {
                (
                    s.puzzle_hash,
                    SyntheticSecretKey::from_synthetic_unchecked(s.sk.clone()),
                )
            })
            .collect(),
    );
    senders.iter().map(|s| (s.puzzle_hash, s.pk)).collect()
}

/// Two XCH inputs, `identical_sends` ordinary sends of the same amount to the
/// same puzzle hash, and one silent payment, bound with
/// `Relation::AssertConcurrent`. Returns the number of intermediate coins in the
/// transaction and the recipient's detections.
fn multi_input_with_identical_sends(
    identical_sends: usize,
) -> Result<(usize, Vec<chia_sdk_driver::silent_payments::DetectedSpCoin>)> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();
    let a = sim.bls(600);
    let b = sim.bls(600);
    let recipient = SilentPaymentKeys::from_mnemonic(&Mnemonic::parse(TEST_MNEMONIC)?);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(b.coin);

    let other: chia_protocol::Bytes32 = [0x77u8; 32].into();
    let mut actions = vec![Action::send(Id::Xch, other, 10, Memos::None); identical_sends];
    actions.push(Action::silent_payment_send(address, 1000, Memos::None));
    let deltas = spends.apply(&mut ctx, &actions)?;

    let ephemeral = spends.xch.items.iter().filter(|i| i.ephemeral).count();

    let pk_map = register_keys(&mut spends, &[&a, &b]);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    sim.spend_coins(ctx.take(), &[a.sk.clone(), b.sk.clone()])?;

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    Ok((ephemeral, detections))
}

/// CHIP-0057 "Inputs for Shared Secret Derivation", sender requirement 2: when
/// the cycle includes an intermediate coin created inside the transaction, that
/// coin's key and coin id are part of the key sum and the coin id set, and the
/// recipient detects the payment.
///
/// With two inputs, the third identical send can no longer be created by either
/// input (each already creates an identical coin), so the action system creates
/// an intermediate coin, which `Relation::AssertConcurrent` binds into the same
/// cycle as the inputs.
#[test]
fn test_intermediate_coins_in_cycle_are_detected() -> Result<()> {
    for identical_sends in 0..=4 {
        let (ephemeral, detections) = multi_input_with_identical_sends(identical_sends)?;
        assert_eq!(
            ephemeral,
            identical_sends.saturating_sub(2),
            "{identical_sends} identical sends: unexpected number of intermediate coins"
        );
        assert_eq!(
            detections.len(),
            1,
            "{identical_sends} identical sends ({ephemeral} intermediate coins): \
             expected exactly one detection"
        );
        assert_eq!(detections[0].amount, 1000);
        assert_eq!(detections[0].k, 0);
        assert!(detections[0].label.is_none());
    }
    Ok(())
}

/// A single input whose change collides with an ordinary send, so that the
/// change has to be created by an intermediate coin that only comes into
/// existence while the transaction is being completed. The outputs must be
/// derived from the group including that coin.
#[test]
fn test_intermediate_coin_created_for_change_is_detected() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    // 1000 in, 100 to the silent payment, 450 back to the sender: the change is
    // also 450 to the sender's puzzle hash, identical to the send.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::send(Id::Xch, sender.puzzle_hash, 450, Memos::None),
            Action::silent_payment_send(address, 100, Memos::None),
        ],
    )?;
    assert_eq!(spends.xch.items.len(), 1, "no intermediate coin yet");

    let pk_map = register_keys(&mut spends, &[&sender]);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    assert_eq!(
        coin_spends.len(),
        2,
        "the change must have required an intermediate coin"
    );
    sim.spend_coins(coin_spends, std::slice::from_ref(&sender.sk))?;

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].amount, 100);

    // The output was derived from the two-coin group, not from either coin on
    // its own (sender requirement 3): neither single-input group finds it.
    let block_outputs = sim.block_outputs(height_before);
    for spend in sim.block_spends(height_before) {
        let single = tweak_data_from_block_spends(std::slice::from_ref(&spend), &block_outputs)?;
        let detections = scan_from_tweaks(
            recipient.scan_sk(),
            recipient.spend_pk(),
            &single,
            None,
            K_MAX_DEFAULT,
        );
        assert!(detections.is_empty());
    }
    Ok(())
}

/// A transaction with one selected input can still spend two coins, when an
/// intermediate coin is needed. Without `Relation::AssertConcurrent` nothing
/// binds the two, so the send must fail instead of producing an output derived
/// from a group no scanner will form.
#[test]
fn test_single_input_with_intermediate_coin_requires_binding() -> Result<()> {
    let (_sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let other: chia_protocol::Bytes32 = [0x77u8; 32].into();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::send(Id::Xch, other, 10, Memos::None),
            Action::send(Id::Xch, other, 10, Memos::None),
            Action::silent_payment_send(address, 100, Memos::None),
        ],
    )?;
    assert_eq!(
        spends.xch.items.iter().filter(|i| i.ephemeral).count(),
        1,
        "the second identical send needs an intermediate coin"
    );

    let pk_map = register_keys(&mut spends, &[&sender]);
    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);
    assert!(
        matches!(result, Err(DriverError::SilentPaymentRequiresInputBinding)),
        "expected SilentPaymentRequiresInputBinding, got {result:?}"
    );
    Ok(())
}

/// An intermediate coin in the spend group whose puzzle hash has no registered
/// secret key fails with a dedicated error, since its key is part of the sum a
/// scanner computes.
#[test]
fn test_intermediate_coin_without_key_errors() -> Result<()> {
    let (_sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address, 100, Memos::None)],
    )?;

    // An intermediate coin locked to a puzzle hash the sender registered no key for.
    let unknown_puzzle_hash: chia_protocol::Bytes32 = [0x55u8; 32].into();
    spends.xch.items.push(FungibleSpend::new(
        Coin::new(sender.coin.coin_id(), unknown_puzzle_hash, 0),
        true,
    ));

    let pk_map = register_keys(&mut spends, &[&sender]);
    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
    assert!(
        matches!(
            result,
            Err(DriverError::SilentPaymentIntermediateKeyMissing)
        ),
        "expected SilentPaymentIntermediateKeyMissing, got {result:?}"
    );
    Ok(())
}

/// `r - sk`, the additive inverse of a secret key mod r.
fn negate(sk: &chia_bls::SecretKey) -> chia_bls::SecretKey {
    let bytes = sk.to_bytes();
    let mut out = [0u8; 32];
    let mut borrow = 0u16;
    for i in (0..32).rev() {
        let lhs = u16::from(GROUP_ORDER[i]);
        let rhs = u16::from(bytes[i]) + borrow;
        if lhs >= rhs {
            out[i] = u8::try_from(lhs - rhs).unwrap();
            borrow = 0;
        } else {
            out[i] = u8::try_from(lhs + 256 - rhs).unwrap();
            borrow = 1;
        }
    }
    chia_bls::SecretKey::from_bytes(&out).expect("r - sk is below r")
}

/// CHIP-0057 "Edge Cases": a spend group whose secret keys sum to zero mod r
/// makes the sender fail. Two inputs with keys `a` and `r - a`.
#[test]
fn test_zero_key_sum_makes_the_sender_fail() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let sk1 = chia_bls::SecretKey::from_seed(&[1u8; 32]);
    let sk2 = negate(&sk1);
    let pk1 = sk1.public_key();
    let pk2 = sk2.public_key();
    assert!((pk1 + &pk2).is_inf(), "the two keys must cancel out");

    let ph1: chia_protocol::Bytes32 = StandardArgs::curry_tree_hash(pk1).into();
    let ph2: chia_protocol::Bytes32 = StandardArgs::curry_tree_hash(pk2).into();
    let coin1 = sim.new_coin(ph1, 600);
    let coin2 = sim.new_coin(ph2, 600);

    let recipient = SilentPaymentKeys::from_mnemonic(&Mnemonic::parse(TEST_MNEMONIC)?);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);

    let mut spends = Spends::new(ph1);
    spends.add(coin1);
    spends.add(coin2);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address, 1000, Memos::None)],
    )?;
    spends.with_silent_payment_keys(
        indexmap! {
            ph1 => SyntheticPublicKey::from_synthetic_unchecked(pk1),
            ph2 => SyntheticPublicKey::from_synthetic_unchecked(pk2),
        },
        indexmap! {
            ph1 => SyntheticSecretKey::from_synthetic_unchecked(sk1),
            ph2 => SyntheticSecretKey::from_synthetic_unchecked(sk2),
        },
    );
    let pk_map = indexmap! { ph1 => pk1, ph2 => pk2 };

    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
    assert!(
        matches!(result, Err(DriverError::SilentPaymentZeroKeySum)),
        "expected SilentPaymentZeroKeySum, got {result:?}"
    );
    assert!(ctx.take().is_empty(), "no coin spends may be produced");
    Ok(())
}

/// Change sent to the wallet's own change address (reserved label m = 0) is
/// detected and reported as label 0 without registering any label, and is
/// spendable with the key derived from the detection (CHIP-0057 "Change
/// Detection").
#[test]
fn test_simulator_e2e_change_address() -> Result<()> {
    let (mut sim, mut ctx, sender, wallet) = setup_e2e()?;
    let change_address = wallet.change_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            change_address,
            250,
            Memos::None,
        )],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        wallet.scan_sk(),
        wallet.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1);
    let detected = &detections[0];
    assert_eq!(detected.label, Some(0), "change is reported as label 0");
    assert_eq!(detected.amount, 250);

    let synthetic_secret = detected.onetime_sk(wallet.spend_sk()).derive_synthetic();
    let conditions = Conditions::new()
        .create_coin(sender.puzzle_hash, detected.amount - 1, Memos::None)
        .reserve_fee(1);
    StandardLayer::new(synthetic_secret.public_key()).spend(
        &mut ctx,
        detected.coin(),
        conditions,
    )?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;
    assert!(
        sim.coin_state(detected.coin_id)
            .expect("detected coin in state")
            .spent_height
            .is_some()
    );
    Ok(())
}

/// CHIP-0057 "Required Behaviors": a spend that asserts a group member's coin
/// id without being asserted back is not part of that group, and the payment is
/// still detected.
///
/// A two-input silent payment and an unrelated third-party spend land in the
/// same block. The third party's coin outputs `ASSERT_CONCURRENT_SPEND` naming
/// one of the sender's coins, which is a one-way edge into the sender's cycle.
#[test]
fn test_one_way_assertion_does_not_join_the_group() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();
    let a = sim.bls(600);
    let b = sim.bls(600);
    let third_party = sim.bls(50);
    let recipient = SilentPaymentKeys::from_mnemonic(&Mnemonic::parse(TEST_MNEMONIC)?);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let height_before = sim.height();

    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(b.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address, 1000, Memos::None)],
    )?;
    let pk_map = register_keys(&mut spends, &[&a, &b]);
    spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;

    // The third party's spend, in the same block.
    StandardLayer::new(third_party.pk).spend(
        &mut ctx,
        third_party.coin,
        Conditions::new()
            .assert_concurrent_spend(a.coin.coin_id())
            .create_coin(third_party.puzzle_hash, 50, Memos::None),
    )?;

    sim.spend_coins(
        ctx.take(),
        &[a.sk.clone(), b.sk.clone(), third_party.sk.clone()],
    )?;
    assert_eq!(sim.block_spends(height_before).len(), 3);

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    assert_eq!(
        tweak_data.tweak_points.len(),
        4,
        "three single-input groups and the sender's two-coin group"
    );
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].amount, 1000);
    Ok(())
}

/// A CAT payment in the simulator, the second oracle of the Extension to CATs
/// (CHIP-0057 "Outputs in CATs", "Matching Outputs under Wrappers").
///
/// A CAT is issued to the sender's standard puzzle in its own transaction.
/// That one CAT coin is then spent alone, an eligible spend (`cat_v2` around
/// the standard puzzle), its standard puzzle creating the bare one-time puzzle
/// hash `p` derived from the coin's own ID and key, hinted `(p)`; the CAT layer
/// wraps it to `wrap(W, p)`.
///
/// Asserts:
/// 1. The block's wrapper list is `[W]`, `W = (asset_id, None)`.
/// 2. One detection: `k = 0`, no label, wrapper `W`, bare puzzle hash `p`,
///    and `coin()` is the CAT coin the simulator created.
/// 3. With the wrapper list cleared the coin is missed.
/// 4. The detected CAT coin is spent with the one-time key through its CAT
///    layer, and the simulator accepts the spend.
#[test]
fn test_simulator_cat_payment_detected_under_wrapper() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let sender_p2 = StandardLayer::new(sender.pk);

    // Issue 1,000 CAT mojos to the sender's standard puzzle.
    let hint = ctx.hint(sender.puzzle_hash)?;
    let (issue_cat, cats) = Cat::single_issuance(
        &mut ctx,
        sender.coin.coin_id(),
        None,
        1_000,
        Conditions::new().create_coin(sender.puzzle_hash, 1_000, hint),
    )?;
    sender_p2.spend(&mut ctx, sender.coin, issue_cat)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;
    let cat = cats[0];
    assert_eq!(cat.info.p2_puzzle_hash, sender.puzzle_hash);
    assert!(sim.coin_state(cat.coin.coin_id()).is_some());
    let w = Wrapper::cat(cat.info.asset_id);
    assert_eq!(cat.coin.puzzle_hash, w.wrap(sender.puzzle_hash));

    // The one-time puzzle hash of k = 0, from the CAT coin's own ID and key:
    // the CAT coin spent alone is a single-input spend group.
    let input_hash = compute_input_hash(&[cat.coin.coin_id()], &sender.pk);
    let p = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &sender.sk,
        &input_hash,
        0,
    )?;

    // Spend that one CAT coin, paying p hinted (p).
    let height_before = sim.height();
    let memos = ctx.hint(p)?;
    let inner = sender_p2
        .spend_with_conditions(&mut ctx, Conditions::new().create_coin(p, 1_000, memos))?;
    let children = Cat::spend_all(&mut ctx, &[CatSpend::new(cat, inner)])?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;
    assert_eq!(children.len(), 1);
    let paid = children[0];
    assert_eq!(paid.coin.puzzle_hash, w.wrap(p));
    assert!(sim.coin_state(paid.coin.coin_id()).is_some());

    // The block of the spend: one tweak point and the wrapper list [W].
    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(tweak_data.tweak_points.len(), 1);

    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1, "expected exactly 1 detection");
    let detected = &detections[0];
    assert_eq!(detected.k, 0);
    assert_eq!(detected.label, None);
    assert_eq!(detected.wrapper, Some(w));
    assert_eq!(detected.puzzle_hash, p, "the bare one-time puzzle hash");
    assert_eq!(
        detected.coin(),
        paid.coin,
        "the CAT coin the simulator created"
    );
    assert_eq!(detected.coin_id, paid.coin.coin_id());
    assert_eq!(detected.amount, 1_000);

    // Without the block's wrapper list the CAT output is missed.
    let mut bare = tweak_data.clone();
    bare.wrappers.clear();
    assert!(
        scan_from_tweaks(
            recipient.scan_sk(),
            recipient.spend_pk(),
            &bare,
            None,
            K_MAX_DEFAULT
        )
        .is_empty(),
        "a CAT output is found only under its wrapper"
    );

    // Spend the detected CAT coin with the one-time key, through its CAT layer.
    let synthetic_secret = detected.onetime_sk(recipient.spend_sk()).derive_synthetic();
    let onetime_p2 = StandardLayer::new(synthetic_secret.public_key());
    let back = ctx.hint(sender.puzzle_hash)?;
    let inner = onetime_p2.spend_with_conditions(
        &mut ctx,
        Conditions::new().create_coin(sender.puzzle_hash, detected.amount, back),
    )?;
    Cat::spend_all(&mut ctx, &[CatSpend::new(paid, inner)])?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;

    let post_state = sim
        .coin_state(detected.coin_id)
        .expect("detected CAT coin in state");
    assert!(
        post_state.spent_height.is_some(),
        "the detected CAT coin must be spent with the one-time key"
    );

    Ok(())
}

// ============================================================================
// Silent payment sends in CATs through the action system (CHIP-0057
// "Spend Groups across Assets", "Outputs in CATs").
// ============================================================================

/// Issues `amount` mojos of a new CAT (revocable with hidden puzzle hash
/// `hidden` when given) to the sender's standard puzzle, spending `xch_coin`
/// of the sender in its own transaction, and returns the CAT coin.
fn issue_cat_to(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    sender: &BlsPairWithCoin,
    xch_coin: Coin,
    hidden: Option<Bytes32>,
    amount: u64,
) -> Result<Cat> {
    Ok(issue_cats_to(sim, ctx, sender, xch_coin, hidden, &[amount])?[0])
}

/// Like [`issue_cat_to`], with one CAT coin of the new asset per amount.
fn issue_cats_to(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    sender: &BlsPairWithCoin,
    xch_coin: Coin,
    hidden: Option<Bytes32>,
    amounts: &[u64],
) -> Result<Vec<Cat>> {
    let hint = ctx.hint(sender.puzzle_hash)?;
    let mut conditions = Conditions::new();
    for &amount in amounts {
        conditions = conditions.create_coin(sender.puzzle_hash, amount, hint);
    }
    let (issue_cat, cats) = Cat::single_issuance(
        ctx,
        xch_coin.coin_id(),
        hidden,
        amounts.iter().sum(),
        conditions,
    )?;
    StandardLayer::new(sender.pk).spend(ctx, xch_coin, issue_cat)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;
    assert_eq!(cats.len(), amounts.len());
    for cat in &cats {
        assert_eq!(cat.info.p2_puzzle_hash, sender.puzzle_hash);
        assert_eq!(cat.info.hidden_puzzle_hash, hidden);
        assert!(sim.coin_state(cat.coin.coin_id()).is_some());
    }
    Ok(cats)
}

/// The conditions output by the spend of `coin_id` among `coin_spends`, from
/// running its full puzzle reveal with its solution as consensus does.
fn run_spend(coin_spends: &[CoinSpend], coin_id: Bytes32) -> Result<(Allocator, Vec<Condition>)> {
    let spend = coin_spends
        .iter()
        .find(|cs| cs.coin.coin_id() == coin_id)
        .expect("the coin is spent in the transaction");
    let mut a = Allocator::new();
    let puzzle = spend.puzzle_reveal.to_clvm(&mut a)?;
    let solution = spend.solution.to_clvm(&mut a)?;
    let output = run_puzzle(&mut a, puzzle, solution)?;
    let conditions = Vec::<Condition>::from_clvm(&a, output)?;
    Ok((a, conditions))
}

/// The number of `ASSERT_CONCURRENT_SPEND` conditions the spend of `coin_id`
/// outputs.
fn concurrent_spend_assertions(coin_spends: &[CoinSpend], coin_id: Bytes32) -> Result<usize> {
    let (_, conditions) = run_spend(coin_spends, coin_id)?;
    Ok(conditions
        .iter()
        .filter(|c| matches!(c, Condition::AssertConcurrentSpend(_)))
        .count())
}

/// The `CREATE_COIN` conditions output by the spend of `coin_id` among
/// `coin_spends`: `(puzzle_hash, amount, memos)`.
fn created_coins(
    coin_spends: &[CoinSpend],
    coin_id: Bytes32,
) -> Result<Vec<(Bytes32, u64, Vec<Bytes>)>> {
    let (a, conditions) = run_spend(coin_spends, coin_id)?;
    let mut created = Vec::new();
    for condition in conditions {
        if let Condition::CreateCoin(cc) = condition {
            let memos = match cc.memos {
                Memos::Some(ptr) => Vec::<Bytes>::from_clvm(&a, ptr)?,
                Memos::None => Vec::new(),
            };
            created.push((cc.puzzle_hash, cc.amount, memos));
        }
    }
    Ok(created)
}

/// Spends a detected CAT coin with the one-time key through its CAT layer (and,
/// for a revocable CAT, the revocation layer's holder path), back to
/// `destination` hinted, and asserts the simulator accepted the spend.
fn spend_detected_cat(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    paid: Cat,
    detected: &chia_sdk_driver::silent_payments::DetectedSpCoin,
    recipient: &SilentPaymentKeys,
    destination: Bytes32,
) -> Result<()> {
    assert_eq!(detected.coin(), paid.coin);
    let synthetic_secret = detected.onetime_sk(recipient.spend_sk()).derive_synthetic();
    let onetime_p2 = StandardLayer::new(synthetic_secret.public_key());
    assert_eq!(
        Bytes32::from(StandardArgs::curry_tree_hash(synthetic_secret.public_key())),
        paid.info.p2_puzzle_hash,
        "the one-time key is the key of the CAT's inner standard puzzle"
    );
    let back = ctx.hint(destination)?;
    let inner = onetime_p2.spend_with_conditions(
        ctx,
        Conditions::new().create_coin(destination, paid.coin.amount, back),
    )?;
    Cat::spend_all(ctx, &[CatSpend::new(paid, inner)])?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;
    let post_state = sim
        .coin_state(paid.coin.coin_id())
        .expect("detected CAT coin in state");
    assert!(
        post_state.spent_height.is_some(),
        "the detected CAT coin must be spent with the one-time key"
    );
    Ok(())
}

/// Plan 15-02 tracer: one CAT coin pays a silent payment through the action
/// system with `Action::silent_payment_send_asset`.
///
/// The CAT coin spent alone is a single-input spend group. Its standard puzzle
/// creates `CREATE_COIN(p, amount, (p))`, `p` derived from that coin's ID and
/// key at `k = 0`; the CAT layer wraps it to `wrap(W, p)`. The block's scan
/// finds it under the wrapper `W`, and it is spent with the one-time key.
#[test]
fn test_simulator_e2e_cat_payment() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);
    let w = Wrapper::cat(cat.info.asset_id);
    let height_before = sim.height();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(cat);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address, 1_000)],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&sender.sk))?;

    // p over [the CAT coin's ID] and its key.
    let input_hash = compute_input_hash(&[cat.coin.coin_id()], &sender.pk);
    let p = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &sender.sk,
        &input_hash,
        0,
    )?;

    // The CAT coin creates exactly the payment: the bare p under the CAT
    // layer, hinted (p), and no change.
    assert_eq!(
        created_coins(&coin_spends, cat.coin.coin_id())?,
        vec![(w.wrap(p), 1_000, vec![Bytes::from(p.to_vec())])]
    );
    let paid = outputs.cats[&id][0];
    assert_eq!(outputs.cats[&id].len(), 1);
    assert_eq!(paid.info.p2_puzzle_hash, p, "the inner puzzle hash is p");
    assert_eq!(paid.coin.puzzle_hash, w.wrap(p));
    assert!(outputs.xch.is_empty());
    assert!(sim.coin_state(paid.coin.coin_id()).is_some());

    // The block of the spend: one tweak point, the wrapper list [W], and the
    // payment detected at k 0 under W.
    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(tweak_data.tweak_points.len(), 1);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1, "expected exactly 1 detection");
    let detected = &detections[0];
    assert_eq!(detected.k, 0);
    assert_eq!(detected.label, None);
    assert_eq!(detected.wrapper, Some(w));
    assert_eq!(detected.puzzle_hash, p);
    assert_eq!(detected.amount, 1_000);

    spend_detected_cat(
        &mut sim,
        &mut ctx,
        paid,
        detected,
        &recipient,
        sender.puzzle_hash,
    )
}

/// The memo list `(p)` of a CAT output.
fn hint_of(p: Bytes32) -> Vec<Bytes> {
    vec![Bytes::from(p.to_vec())]
}

/// Scans the block at `height` for the recipient of `setup_e2e`.
fn scan_block(
    sim: &Simulator,
    height: u32,
    recipient: &SilentPaymentKeys,
) -> (
    chia_sdk_driver::silent_payments::TweakData,
    Vec<chia_sdk_driver::silent_payments::DetectedSpCoin>,
) {
    let tweak_data = tweak_data_from_simulator_block(sim, height);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &tweak_data,
        None,
        K_MAX_DEFAULT,
    );
    (tweak_data, detections)
}

/// One XCH coin of sender `a` and one CAT coin of sender `b` (different keys,
/// the shape of Test Vector 10), bound by `Relation::AssertConcurrent`, paying
/// one recipient 1,000 in the CAT and 2,000 in XCH.
///
/// `cat_action_first` puts the CAT action before the XCH action (Vector 10's
/// order); `cat_added_first` adds the CAT coin to `Spends` before the XCH coin.
/// Returns the one-time puzzle hashes `[p_0, p_1]` the test derives from both
/// coins with the protocol functions.
fn xch_and_cat_payment(cat_action_first: bool, cat_added_first: bool) -> Result<[Bytes32; 2]> {
    let (mut sim, mut ctx, b, recipient) = setup_e2e()?;
    let a = sim.bls(2_000);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &b, b.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);
    let w = Wrapper::cat(cat.info.asset_id);
    let height_before = sim.height();

    let mut spends = Spends::new(a.puzzle_hash);
    if cat_added_first {
        spends.add(cat);
        spends.add(a.coin);
    } else {
        spends.add(a.coin);
        spends.add(cat);
    }
    let cat_action = Action::silent_payment_send_asset(id, address.clone(), 1_000);
    let xch_action = Action::silent_payment_send(address, 2_000, Memos::None);
    let actions = if cat_action_first {
        vec![cat_action, xch_action]
    } else {
        vec![xch_action, cat_action]
    };
    let deltas = spends.apply(&mut ctx, &actions)?;
    let pk_map = register_keys(&mut spends, &[&a, &b]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), &[a.sk.clone(), b.sk.clone()])?;

    // The two coins are bound into one 2-cycle.
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, a.coin.coin_id())?,
        1
    );
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, cat.coin.coin_id())?,
        1
    );

    // One key sum and one input hash over both coins, whatever their asset.
    let coin_ids = [a.coin.coin_id(), cat.coin.coin_id()];
    let a_sum = aggregate_sender_sks(&[a.sk.clone(), b.sk.clone()])?;
    let input_hash = compute_input_hash(&coin_ids, &a_sum.public_key());
    let p: [Bytes32; 2] = [
        derive_one_time_puzzle_hash(
            recipient.scan_pk(),
            recipient.spend_pk(),
            &a_sum,
            &input_hash,
            0,
        )?,
        derive_one_time_puzzle_hash(
            recipient.scan_pk(),
            recipient.spend_pk(),
            &a_sum,
            &input_hash,
            1,
        )?,
    ];
    let (k_cat, k_xch) = if cat_action_first { (0, 1) } else { (1, 0) };

    // The CAT coin creates the CAT output (hinted), the XCH coin the XCH
    // output (no memo); neither has change.
    assert_eq!(
        created_coins(&coin_spends, cat.coin.coin_id())?,
        vec![(w.wrap(p[k_cat]), 1_000, hint_of(p[k_cat]))]
    );
    assert_eq!(
        created_coins(&coin_spends, a.coin.coin_id())?,
        vec![(p[k_xch], 2_000, Vec::new())]
    );
    assert_eq!(outputs.cats[&id].len(), 1);
    assert_eq!(outputs.cats[&id][0].info.p2_puzzle_hash, p[k_cat]);
    assert_eq!(
        outputs.xch,
        vec![Coin::new(a.coin.coin_id(), p[k_xch], 2_000)]
    );

    // Both outputs are found from the block, one k across the two assets.
    let (tweak_data, detections) = scan_block(&sim, height_before, &recipient);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(detections.len(), 2, "both outputs are detected");
    let cat_detection = detections
        .iter()
        .find(|d| d.wrapper.is_some())
        .expect("the CAT output");
    let xch_detection = detections
        .iter()
        .find(|d| d.wrapper.is_none())
        .expect("the XCH output");
    assert_eq!(
        (
            cat_detection.k,
            cat_detection.wrapper,
            cat_detection.puzzle_hash
        ),
        (u32::try_from(k_cat)?, Some(w), p[k_cat])
    );
    assert_eq!(
        (
            xch_detection.k,
            xch_detection.puzzle_hash,
            xch_detection.amount
        ),
        (u32::try_from(k_xch)?, p[k_xch], 2_000)
    );

    // From the group's one tweak point alone, both are found.
    let tweak_point = compute_tweak_point(&coin_ids, &a_sum.public_key()).expect("tweak point");
    let one_point = chia_sdk_driver::silent_payments::TweakData::with_wrappers(
        vec![tweak_point],
        tweak_data.outputs.clone(),
        tweak_data.wrappers.clone(),
    );
    let found = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        &one_point,
        None,
        K_MAX_DEFAULT,
    );
    assert_eq!(found.len(), 2, "one tweak point finds both outputs");

    // Both are spent with their one-time keys.
    spend_detected_cat(
        &mut sim,
        &mut ctx,
        outputs.cats[&id][0],
        cat_detection,
        &recipient,
        a.puzzle_hash,
    )?;
    let synthetic_secret = xch_detection
        .onetime_sk(recipient.spend_sk())
        .derive_synthetic();
    StandardLayer::new(synthetic_secret.public_key()).spend(
        &mut ctx,
        xch_detection.coin(),
        Conditions::new().create_coin(a.puzzle_hash, 2_000, Memos::None),
    )?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;
    assert!(
        sim.coin_state(xch_detection.coin_id)
            .expect("XCH output in state")
            .spent_height
            .is_some()
    );

    Ok(p)
}

/// CHIP-0057 Test Vector 10 shape through the action system: an XCH coin and
/// a CAT coin bound by `Relation::AssertConcurrent` form one spend group, with
/// one key sum, one input hash and one tweak point, and outputs in both assets
/// to one scan key share one sequence of `k`: the CAT output at k 0, the XCH
/// output at k 1.
#[test]
fn test_xch_and_cat_coins_form_one_group_with_one_k() -> Result<()> {
    let p = xch_and_cat_payment(true, false)?;
    assert_ne!(p[0], p[1]);
    Ok(())
}

/// `k` follows the order of the actions per scan key across assets: with the
/// XCH action first the XCH output is k 0 and the CAT output k 1. The order in
/// which the coins are added to `Spends` does not change the group (the
/// post-check compares it as a set) or the outputs.
#[test]
fn test_cat_order_of_actions_sets_k() -> Result<()> {
    let cat_first = xch_and_cat_payment(true, false)?;
    let xch_first = xch_and_cat_payment(false, false)?;
    let xch_first_cat_added_first = xch_and_cat_payment(false, true)?;
    let cat_first_cat_added_first = xch_and_cat_payment(true, true)?;
    // Same coins and keys in every run (deterministic simulator), so the same
    // two one-time puzzle hashes, assigned to the assets by action order.
    assert_eq!(cat_first, xch_first);
    assert_eq!(cat_first, xch_first_cat_added_first);
    assert_eq!(cat_first, cat_first_cat_added_first);
    Ok(())
}

/// RESEARCH Pitfall 1 (SDK-01 adjacency): with one XCH coin and two CAT coins
/// c1, c2 of one asset, the `ASSERT_CONCURRENT_SPEND` cycle binds the XCH coin
/// and c1 (the first conditions spend of the asset); c2 is tied to c1 by the
/// CAT ring only. The spend group is {XCH, c1}, the CAT output is created by
/// c1, and the payment is detected. Deriving over {XCH, c1, c2} would give a
/// different puzzle hash.
#[test]
fn test_two_cat_coins_of_one_asset_group_with_the_first() -> Result<()> {
    let (mut sim, mut ctx, b, recipient) = setup_e2e()?;
    let a = sim.bls(500);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cats = issue_cats_to(&mut sim, &mut ctx, &b, b.coin, None, &[600, 400])?;
    let (c1, c2) = (cats[0], cats[1]);
    let id = Id::Existing(c1.info.asset_id);
    let w = Wrapper::cat(c1.info.asset_id);
    let height_before = sim.height();

    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(c1);
    spends.add(c2);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address, 1_000)],
    )?;
    let pk_map = register_keys(&mut spends, &[&a, &b]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), &[a.sk.clone(), b.sk.clone()])?;

    // The cycle binds the XCH coin and c1 only.
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, a.coin.coin_id())?,
        1
    );
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, c1.coin.coin_id())?,
        1
    );
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, c2.coin.coin_id())?,
        0
    );

    let group = [a.coin.coin_id(), c1.coin.coin_id()];
    let a_sum = aggregate_sender_sks(&[a.sk.clone(), b.sk.clone()])?;
    let p = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &a_sum,
        &compute_input_hash(&group, &a_sum.public_key()),
        0,
    )?;
    let all_three = [a.coin.coin_id(), c1.coin.coin_id(), c2.coin.coin_id()];
    let a_sum_3 = aggregate_sender_sks(&[a.sk.clone(), b.sk.clone(), b.sk.clone()])?;
    let p_all_three = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &a_sum_3,
        &compute_input_hash(&all_three, &a_sum_3.public_key()),
        0,
    )?;
    assert_ne!(p, p_all_three);

    // c1 creates the output; c2 creates nothing (the ring carries its value).
    assert_eq!(
        created_coins(&coin_spends, c1.coin.coin_id())?,
        vec![(w.wrap(p), 1_000, hint_of(p))]
    );
    assert!(created_coins(&coin_spends, c2.coin.coin_id())?.is_empty());
    assert_eq!(outputs.cats[&id].len(), 1);
    assert_eq!(
        outputs.cats[&id][0].coin.parent_coin_info,
        c1.coin.coin_id()
    );

    let (_, detections) = scan_block(&sim, height_before, &recipient);
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].k, 0);
    assert_eq!(detections[0].wrapper, Some(w));
    assert_eq!(detections[0].puzzle_hash, p);
    spend_detected_cat(
        &mut sim,
        &mut ctx,
        outputs.cats[&id][0],
        &detections[0],
        &recipient,
        a.puzzle_hash,
    )
}

/// A CAT-only group of two assets: one coin of each, bound in a cycle, with
/// one output in each asset to one scan key, k 0 in the first asset and k 1 in
/// the second, both detected from one group under their wrappers.
#[test]
fn test_cat_only_group_of_two_assets() -> Result<()> {
    let (mut sim, mut ctx, b, recipient) = setup_e2e()?;
    let second_xch = sim.new_coin(b.puzzle_hash, 500);
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat1 = issue_cat_to(&mut sim, &mut ctx, &b, b.coin, None, 1_000)?;
    let cat2 = issue_cat_to(&mut sim, &mut ctx, &b, second_xch, None, 500)?;
    assert_ne!(cat1.info.asset_id, cat2.info.asset_id);
    let (id1, id2) = (
        Id::Existing(cat1.info.asset_id),
        Id::Existing(cat2.info.asset_id),
    );
    let (w1, w2) = (
        Wrapper::cat(cat1.info.asset_id),
        Wrapper::cat(cat2.info.asset_id),
    );
    let height_before = sim.height();

    let mut spends = Spends::new(b.puzzle_hash);
    spends.add(cat1);
    spends.add(cat2);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::silent_payment_send_asset(id1, address.clone(), 1_000),
            Action::silent_payment_send_asset(id2, address, 500),
        ],
    )?;
    let pk_map = register_keys(&mut spends, &[&b]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&b.sk))?;

    assert_eq!(
        concurrent_spend_assertions(&coin_spends, cat1.coin.coin_id())?,
        1
    );
    assert_eq!(
        concurrent_spend_assertions(&coin_spends, cat2.coin.coin_id())?,
        1
    );

    // One term per coin, even though both coins have the same key.
    let coin_ids = [cat1.coin.coin_id(), cat2.coin.coin_id()];
    let a_sum = aggregate_sender_sks(&[b.sk.clone(), b.sk.clone()])?;
    let input_hash = compute_input_hash(&coin_ids, &a_sum.public_key());
    let p0 = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &a_sum,
        &input_hash,
        0,
    )?;
    let p1 = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &a_sum,
        &input_hash,
        1,
    )?;
    assert_eq!(
        created_coins(&coin_spends, cat1.coin.coin_id())?,
        vec![(w1.wrap(p0), 1_000, hint_of(p0))]
    );
    assert_eq!(
        created_coins(&coin_spends, cat2.coin.coin_id())?,
        vec![(w2.wrap(p1), 500, hint_of(p1))]
    );

    let (tweak_data, mut detections) = scan_block(&sim, height_before, &recipient);
    assert_eq!(
        tweak_data.wrappers,
        chia_sdk_driver::silent_payments::sort_wrappers(vec![w1, w2])
    );
    detections.sort_by_key(|d| d.k);
    assert_eq!(detections.len(), 2);
    assert_eq!(
        (
            detections[0].k,
            detections[0].wrapper,
            detections[0].puzzle_hash
        ),
        (0, Some(w1), p0)
    );
    assert_eq!(
        (
            detections[1].k,
            detections[1].wrapper,
            detections[1].puzzle_hash
        ),
        (1, Some(w2), p1)
    );
    spend_detected_cat(
        &mut sim,
        &mut ctx,
        outputs.cats[&id1][0],
        &detections[0],
        &recipient,
        b.puzzle_hash,
    )?;
    spend_detected_cat(
        &mut sim,
        &mut ctx,
        outputs.cats[&id2][0],
        &detections[1],
        &recipient,
        b.puzzle_hash,
    )
}

/// A revocable CAT (CHIP-0038) spent on the holder's path is an eligible
/// spend: its silent payment output keeps the revocation layer, so the coin's
/// puzzle hash is `Wrapper::revocable(asset_id, hidden).wrap(p)`; it is
/// detected under that wrapper and spent through the holder path with the
/// one-time key.
#[test]
fn test_revocable_cat_holder_path_payment() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let hidden = Bytes32::new([7; 32]);

    // Issue the revocable CAT with the action system.
    let hint = ctx.hint(sender.puzzle_hash)?;
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::single_issue_cat(Some(hidden), 1_000),
            Action::send(Id::New(0), sender.puzzle_hash, 1_000, hint),
        ],
    )?;
    let issued = spends.finish_with_keys(
        &mut ctx,
        &deltas,
        Relation::None,
        &indexmap! { sender.puzzle_hash => sender.pk },
    )?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;
    let cat = issued.cats[&Id::New(0)][0];
    assert_eq!(cat.info.hidden_puzzle_hash, Some(hidden));
    assert_eq!(cat.info.p2_puzzle_hash, sender.puzzle_hash);
    let id = Id::Existing(cat.info.asset_id);
    let w = Wrapper::revocable(cat.info.asset_id, hidden);
    assert_eq!(cat.coin.puzzle_hash, w.wrap(sender.puzzle_hash));
    let height_before = sim.height();

    // Pay it silently, on the holder's path (an ordinary `add`).
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(cat);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address, 1_000)],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&sender.sk))?;

    let p = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &sender.sk,
        &compute_input_hash(&[cat.coin.coin_id()], &sender.pk),
        0,
    )?;
    assert_eq!(
        created_coins(&coin_spends, cat.coin.coin_id())?,
        vec![(w.wrap(p), 1_000, hint_of(p))],
        "the output keeps the revocation layer and is hinted (p)"
    );
    let paid = outputs.cats[&id][0];
    assert_eq!(paid.info.hidden_puzzle_hash, Some(hidden));
    assert_eq!(paid.info.p2_puzzle_hash, p);
    assert_eq!(paid.coin.puzzle_hash, w.wrap(p));

    let (tweak_data, detections) = scan_block(&sim, height_before, &recipient);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].k, 0);
    assert_eq!(detections[0].wrapper, Some(w));
    assert_eq!(detections[0].puzzle_hash, p);
    spend_detected_cat(
        &mut sim,
        &mut ctx,
        paid,
        &detections[0],
        &recipient,
        sender.puzzle_hash,
    )
}

/// CHIP-0057 and CONTEXT D-04: the change of a CAT input goes to the sender's
/// change puzzle hash as an ordinary CAT, hinted with that puzzle hash, and is
/// not a silent payment output.
#[test]
fn test_cat_change_is_a_standard_cat_change() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);
    let w = Wrapper::cat(cat.info.asset_id);
    let height_before = sim.height();

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(cat);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address, 600)],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&sender.sk))?;

    let p = derive_one_time_puzzle_hash(
        recipient.scan_pk(),
        recipient.spend_pk(),
        &sender.sk,
        &compute_input_hash(&[cat.coin.coin_id()], &sender.pk),
        0,
    )?;
    assert_eq!(
        created_coins(&coin_spends, cat.coin.coin_id())?,
        vec![
            (w.wrap(p), 600, hint_of(p)),
            (w.wrap(sender.puzzle_hash), 400, hint_of(sender.puzzle_hash)),
        ]
    );
    let created = &outputs.cats[&id];
    assert_eq!(created.len(), 2);
    assert_eq!(created[1].info.p2_puzzle_hash, sender.puzzle_hash);
    assert_eq!(created[1].coin.amount, 400);

    // Only the payment is a silent payment output.
    let (_, detections) = scan_block(&sim, height_before, &recipient);
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].amount, 600);
    assert_eq!(detections[0].puzzle_hash, p);
    Ok(())
}

/// SDK-01 edges of the group's size. A CAT coin spent alone is a
/// single-input spend group and is accepted (also covered end to end by
/// `test_simulator_e2e_cat_payment`). A send with no eligible coin fails:
///
/// - With no coin at all, an XCH send fails at apply time with the ordinary
///   coin-selection error `NoSourceForOutput`, and a CAT send with
///   `SilentPaymentAssetNotInGroup`.
/// - With only a settlement coin, an XCH send fails at apply time with
///   `SilentPaymentParentNotEligible`.
/// - If the only coin of the group stops being an eligible spend after the
///   action was applied (here its spend kind is replaced by a settlement spend
///   through the public field), `prepare` finds an empty group and fails with
///   `SilentPaymentNoXchInputs`, whose message still contains "requires an xch
///   input".
#[test]
fn test_cat_only_single_coin_group_is_accepted_and_empty_group_fails() -> Result<()> {
    use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
    use chia_sdk_driver::SpendKind;

    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);

    // A CAT-only group of one coin is accepted.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(cat);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(
            id,
            address.clone(),
            1_000,
        )],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let outputs = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;
    assert_eq!(outputs.cats[&id].len(), 1);
    ctx.take();

    // No coin at all.
    let mut spends = Spends::new(sender.puzzle_hash);
    let result = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address.clone(), 1, Memos::None)],
    );
    assert!(
        matches!(result, Err(DriverError::NoSourceForOutput)),
        "got {result:?}"
    );
    let result = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address.clone(), 1)],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentAssetNotInGroup)),
        "got {result:?}"
    );

    // Only a settlement coin.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(Coin::new(
        Bytes32::new([1; 32]),
        SETTLEMENT_PAYMENT_HASH.into(),
        1_000,
    ));
    let result = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address.clone(), 1, Memos::None)],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentParentNotEligible)),
        "got {result:?}"
    );

    // The group's only coin is no longer a conditions spend at prepare time.
    let xch = sim.new_coin(sender.puzzle_hash, 1_000);
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(xch);
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(address, 1_000, Memos::None)],
    )?;
    spends.xch.items[0].kind = SpendKind::settlement();
    let pk_map = register_keys(&mut spends, &[&sender]);
    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);
    let Err(error) = result else {
        panic!("an empty group must fail");
    };
    assert!(
        matches!(error, DriverError::SilentPaymentNoXchInputs),
        "got {error:?}"
    );
    assert!(error.to_string().contains("requires an xch input"));
    Ok(())
}

// ============================================================================
// Rejections of spends outside the CHIP's closed list (CHIP-0057 "Eligible
// Spends", "Outputs in CATs"; RESEARCH RQ-4, RQ-5).
// ============================================================================

/// RQ-4: a CAT issued in the same transaction cannot join a silent payment
/// spend group. With an XCH silent payment the transaction is rejected at
/// prepare with `SilentPaymentNewCatUnsupported`, before any key work; a
/// silent payment in the new CAT itself is rejected when the action is applied.
#[test]
fn test_cat_issued_in_the_same_transaction_is_rejected() -> Result<()> {
    let (_sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::single_issue_cat(None, 1),
            Action::silent_payment_send(address.clone(), 100, Memos::None),
        ],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
    assert!(
        matches!(result, Err(DriverError::SilentPaymentNewCatUnsupported)),
        "got {result:?}"
    );
    assert!(ctx.take().is_empty(), "nothing is emitted");

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let result = spends.apply(
        &mut ctx,
        &[
            Action::single_issue_cat(None, 100),
            Action::silent_payment_send_asset(Id::New(0), address, 100),
        ],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentNewCatUnsupported)),
        "got {result:?}"
    );
    Ok(())
}

/// A revocable CAT spent through the issuer's hidden puzzle
/// (`Spends::add_for_revocation`) is not an eligible spend: an XCH silent
/// payment in the same `Spends` is rejected at prepare, and a silent payment
/// in that CAT is rejected when the action is applied, both with
/// `SilentPaymentIssuerPathSpend`.
#[test]
fn test_issuer_path_revocable_cat_is_rejected() -> Result<()> {
    let (mut sim, mut ctx, issuer, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    // The hidden puzzle is the issuer's own standard puzzle.
    let cat = issue_cat_to(
        &mut sim,
        &mut ctx,
        &issuer,
        issuer.coin,
        Some(issuer.puzzle_hash),
        1_000,
    )?;
    let id = Id::Existing(cat.info.asset_id);
    let xch = sim.new_coin(issuer.puzzle_hash, 1_000);

    let mut spends = Spends::new(issuer.puzzle_hash);
    spends.add(xch);
    spends.add_for_revocation(cat)?;
    let deltas = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send(
            address.clone(),
            100,
            Memos::None,
        )],
    )?;
    let pk_map = register_keys(&mut spends, &[&issuer]);
    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
    assert!(
        matches!(result, Err(DriverError::SilentPaymentIssuerPathSpend)),
        "got {result:?}"
    );
    assert!(ctx.take().is_empty(), "nothing is emitted");

    let mut spends = Spends::new(issuer.puzzle_hash);
    spends.add_for_revocation(cat)?;
    let result = spends.apply(
        &mut ctx,
        &[Action::silent_payment_send_asset(id, address, 100)],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentIssuerPathSpend)),
        "got {result:?}"
    );
    Ok(())
}

/// CHIP-0057 Required Behaviors (Sending with CATs): an output in an asset
/// that no coin of the spend group holds makes the sender fail, with
/// `SilentPaymentAssetNotInGroup` when the action is applied: in a `Spends`
/// with XCH only, and in one holding a different CAT.
#[test]
fn test_output_in_an_asset_outside_the_group_is_rejected() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let xch = sim.new_coin(sender.puzzle_hash, 1_000);
    let elsewhere = Id::Existing(Bytes32::new([0x99; 32]));
    assert_ne!(elsewhere, Id::Existing(cat.info.asset_id));

    let mut xch_only = Spends::new(sender.puzzle_hash);
    xch_only.add(xch);
    let mut other_cat = Spends::new(sender.puzzle_hash);
    other_cat.add(xch);
    other_cat.add(cat);
    for mut spends in [xch_only, other_cat] {
        let result = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send_asset(
                elsewhere,
                address.clone(),
                100,
            )],
        );
        assert!(
            matches!(result, Err(DriverError::SilentPaymentAssetNotInGroup)),
            "got {result:?}"
        );
    }
    Ok(())
}

/// A silent payment CAT output carries exactly the SDK-built hint `(p)`:
/// caller memos (of any shape) are refused with `SilentPaymentCatMemos` when
/// the action is applied.
#[test]
fn test_caller_memos_on_a_cat_output_are_rejected() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);

    let hint = ctx.hint(sender.puzzle_hash)?;
    let short = ctx.memos(&[Bytes::from(vec![1u8, 2, 3])])?;
    for memos in [hint, short] {
        let mut spends = Spends::new(sender.puzzle_hash);
        spends.add(cat);
        let action = Action::SilentPaymentSend(SilentPaymentSendAction {
            id,
            recipient: address.clone(),
            amount: 100,
            memos,
        });
        let result = spends.apply(&mut ctx, &[action]);
        assert!(
            matches!(result, Err(DriverError::SilentPaymentCatMemos)),
            "got {result:?}"
        );
    }
    Ok(())
}

/// The 32-byte first-memo guard on the caller memos of an XCH silent payment
/// output still applies, also when the transaction spends a CAT and pays a
/// silent payment in it.
#[test]
fn test_xch_memo_hint_guard_still_applies() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let cat = issue_cat_to(&mut sim, &mut ctx, &sender, sender.coin, None, 1_000)?;
    let id = Id::Existing(cat.info.asset_id);
    let xch = sim.new_coin(sender.puzzle_hash, 1_000);

    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(xch);
    spends.add(cat);
    let hint = ctx.hint(Bytes32::new([0x55; 32]))?;
    let result = spends.apply(
        &mut ctx,
        &[
            Action::silent_payment_send_asset(id, address.clone(), 100),
            Action::silent_payment_send(address, 100, hint),
        ],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentMemoHintForbidden)),
        "got {result:?}"
    );
    Ok(())
}

// ============================================================================
// Silent payments through settlement coins (CHIP-0057 "Payments through
// Settlement Coins").
// ============================================================================

/// The `ASSERT_PUZZLE_ANNOUNCEMENT` conditions output by the spends of
/// `coin_ids` among `coin_spends`, run as consensus does.
fn puzzle_announcement_assertions(
    coin_spends: &[CoinSpend],
    coin_ids: &[Bytes32],
) -> Result<Vec<Bytes32>> {
    let mut asserted = Vec::new();
    for &coin_id in coin_ids {
        let (_, conditions) = run_spend(coin_spends, coin_id)?;
        for condition in conditions {
            if let Condition::AssertPuzzleAnnouncement(a) = condition {
                asserted.push(a.announcement_id);
            }
        }
    }
    Ok(asserted)
}

/// The parties of an offer made and taken in one simulator block, and who is
/// paid silently.
#[derive(Clone, Copy)]
struct OfferCase {
    /// The maker requests its CAT silently and pays its XCH change to its
    /// change address; otherwise it requests the CAT at its own puzzle hash.
    maker_silent: bool,
    /// The taker takes the offered XCH silently and pays its CAT change to its
    /// change address; otherwise both go to its own puzzle hash.
    taker_silent: bool,
    /// The hidden puzzle hash of the requested CAT, for a revocable CAT.
    hidden: Option<Bytes32>,
    /// A payment of the taker in XCH to an address that is not a silent payment
    /// address, `(puzzle_hash, amount)`, taken out of the offered XCH next to
    /// its silent payment; its action is applied before the silent one.
    taker_other: Option<(Bytes32, u64)>,
}

/// An offer made and taken in one simulator block.
///
/// The maker offers 1,000 mojos of XCH from two coins bound in a cycle and
/// requests 1,000 of a CAT; its change is 200. The taker gives the CAT from one
/// coin of 1,500; its CAT change is 500.
struct OfferBlock {
    sim: Simulator,
    ctx: SpendContext,
    /// The height of the block in which the offer was taken.
    height: u32,
    /// The requested CAT's wrapper.
    w: Wrapper,
    /// The offer file (input spends and requested payments).
    offer_file: SpendBundle,
    maker: SilentPaymentKeys,
    /// The two XCH coins the maker spends, bound in a cycle.
    maker_coins: [BlsPairWithCoin; 2],
    /// The maker's coin spends (the input spend bundle of the offer).
    maker_coin_spends: Vec<CoinSpend>,
    maker_outputs: chia_sdk_driver::Outputs,
    /// `[p_0, p_1]` of the maker: the CAT payment at k 0 and the change at
    /// k 1, derived here from the maker's coins with the protocol functions.
    maker_p: [Bytes32; 2],
    /// The CAT coin the taker's settlement coin paid the maker.
    maker_paid: Cat,
    taker: BlsPairWithCoin,
    taker_keys: SilentPaymentKeys,
    /// The CAT coin the taker spends.
    taker_cat: Cat,
    /// The XCH settlement coin the maker offered.
    offered_xch: Coin,
    taker_coin_spends: Vec<CoinSpend>,
    taker_outputs: chia_sdk_driver::Outputs,
    taker_nonce: Bytes32,
    /// `[p_0, p_1]` of the taker: the XCH payment at k 0 and the CAT change at
    /// k 1, derived here from the taker's coin.
    taker_p: [Bytes32; 2],
}

/// The recipient keys of the taker (another wallet than the maker's).
fn taker_keys() -> Result<SilentPaymentKeys> {
    Ok(SilentPaymentKeys::from_mnemonic(&Mnemonic::from_entropy(
        &[0x5a; 16],
    )?))
}

/// Makes and takes an offer in one block. The maker's offer is built from its
/// `Outputs` (with `Outputs::silent_payment_requests` and
/// `Outputs::silent_payment_asset_info` when it is paid silently), encoded as
/// an offer file and parsed back; the taker takes it with the SDK's ordinary
/// flow (`offer.requested_payments().actions()`), plus its own silent actions.
fn offer_block(case: OfferCase) -> Result<OfferBlock> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();
    let maker = SilentPaymentKeys::from_mnemonic(&Mnemonic::parse(TEST_MNEMONIC)?);
    let taker_keys = taker_keys()?;
    let a = sim.bls(700);
    let b = sim.bls(500);
    let taker = sim.bls(1_500);
    let taker_cat = issue_cat_to(&mut sim, &mut ctx, &taker, taker.coin, case.hidden, 1_500)?;
    let w = Wrapper {
        asset_id: taker_cat.info.asset_id,
        hidden_puzzle_hash: case.hidden,
    };
    let id = Id::Existing(w.asset_id);
    let height = sim.height();

    // The maker's transaction.
    let maker_nonce = Offer::nonce(vec![a.coin.coin_id(), b.coin.coin_id()]);
    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(b.coin);
    let mut actions = vec![Action::send(
        Id::Xch,
        SETTLEMENT_PAYMENT_HASH.into(),
        1_000,
        Memos::None,
    )];
    let mut requested = RequestedPayments::new();
    let mut asset_info = AssetInfo::new();
    if case.maker_silent {
        actions.push(Action::silent_payment_requested(
            Some(w),
            maker.unlabeled_address(SilentPaymentNetwork::Testnet),
            1_000,
            maker_nonce,
        ));
        actions.push(Action::silent_payment_send(
            maker.change_address(SilentPaymentNetwork::Testnet),
            200,
            Memos::None,
        ));
    } else {
        let hint = ctx.hint(a.puzzle_hash)?;
        requested.cats.insert(
            w.asset_id,
            vec![NotarizedPayment::new(
                maker_nonce,
                vec![Payment::new(a.puzzle_hash, 1_000, hint)],
            )],
        );
        asset_info.insert_cat(w.asset_id, CatAssetInfo::new(case.hidden))?;
        spends.conditions.required = spends
            .conditions
            .required
            .extend(requested.assertions(&mut ctx, &asset_info)?);
    }
    let deltas = spends.apply(&mut ctx, &actions)?;
    let pk_map = register_keys(&mut spends, &[&a, &b]);
    let maker_outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let maker_coin_spends = ctx.take();
    if case.maker_silent {
        requested = maker_outputs.silent_payment_requests.clone();
        asset_info = maker_outputs.silent_payment_asset_info.clone();
    }
    let signature = sign_transaction(&maker_coin_spends, &[a.sk.clone(), b.sk.clone()])?;
    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        SpendBundle::new(maker_coin_spends.clone(), signature),
        requested,
        asset_info,
    )?;

    // The offer file lists the requested payments; the taker parses it back.
    let offer_file = offer.to_spend_bundle(&mut ctx)?;
    let offer = Offer::from_spend_bundle(&mut ctx, &offer_file)?;
    let offered_xch = offer.offered_coins().xch[0];
    assert_eq!(offered_xch.puzzle_hash, SETTLEMENT_PAYMENT_HASH.into());

    // The taker's transaction.
    let taker_nonce = Offer::nonce(vec![taker_cat.coin.coin_id()]);
    let mut spends = Spends::new(taker.puzzle_hash);
    spends.add(offer.offered_coins().clone());
    spends.add(taker_cat);
    let mut actions = offer.requested_payments().actions();
    let mut xch_amount = 1_000;
    if let Some((q, amount)) = case.taker_other {
        xch_amount -= amount;
        actions.push(Action::silent_payment_settle_other(
            None,
            Payment::new(q, amount, Memos::Some(clvmr::NodePtr::NIL)),
            taker_nonce,
        ));
    }
    if case.taker_silent {
        actions.push(Action::silent_payment_settle(
            None,
            taker_keys.unlabeled_address(SilentPaymentNetwork::Testnet),
            xch_amount,
            taker_nonce,
        ));
        actions.push(Action::silent_payment_send_asset(
            id,
            taker_keys.change_address(SilentPaymentNetwork::Testnet),
            500,
        ));
    }
    let deltas = spends.apply(&mut ctx, &actions)?;
    let taker_pk_map = register_keys(&mut spends, &[&taker]);
    let taker_outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &taker_pk_map)?;
    let taker_coin_spends = ctx.take();
    let signature = sign_transaction(&taker_coin_spends, std::slice::from_ref(&taker.sk))?;
    sim.new_transaction(offer.take(SpendBundle::new(taker_coin_spends.clone(), signature)))?;

    // The parties' one-time puzzle hashes, with the protocol functions.
    let coin_ids = [a.coin.coin_id(), b.coin.coin_id()];
    let a_sum = aggregate_sender_sks(&[a.sk.clone(), b.sk.clone()])?;
    let input_hash = compute_input_hash(&coin_ids, &a_sum.public_key());
    let maker_p = [
        derive_one_time_puzzle_hash(maker.scan_pk(), maker.spend_pk(), &a_sum, &input_hash, 0)?,
        derive_one_time_puzzle_hash(
            maker.scan_pk(),
            &maker.change_address(SilentPaymentNetwork::Testnet).spend_pk,
            &a_sum,
            &input_hash,
            1,
        )?,
    ];
    let taker_input_hash = compute_input_hash(&[taker_cat.coin.coin_id()], &taker.pk);
    let taker_p = [
        derive_one_time_puzzle_hash(
            taker_keys.scan_pk(),
            taker_keys.spend_pk(),
            &taker.sk,
            &taker_input_hash,
            0,
        )?,
        derive_one_time_puzzle_hash(
            taker_keys.scan_pk(),
            &taker_keys
                .change_address(SilentPaymentNetwork::Testnet)
                .spend_pk,
            &taker.sk,
            &taker_input_hash,
            1,
        )?,
    ];
    let maker_paid_p2 = if case.maker_silent {
        maker_p[0]
    } else {
        a.puzzle_hash
    };
    let maker_paid = *taker_outputs.cats[&id]
        .iter()
        .find(|c| c.info.p2_puzzle_hash == maker_paid_p2)
        .expect("the taker's settlement coin pays the maker");

    Ok(OfferBlock {
        sim,
        ctx,
        height,
        w,
        offer_file,
        maker,
        maker_coins: [a, b],
        maker_coin_spends,
        maker_outputs,
        maker_p,
        maker_paid,
        taker,
        taker_keys,
        taker_cat,
        offered_xch,
        taker_coin_spends,
        taker_outputs,
        taker_nonce,
        taker_p,
    })
}

/// Checks the maker's side of an offer block in which the maker is paid
/// silently: the maker's transaction asserts exactly one puzzle announcement,
/// that of the CAT settlement coin for the notarized payment
/// `(nonce . ((p_0 1000 (p_0))))` published in `Outputs::silent_payment_requests`;
/// the maker's scan of the block finds the CAT payment at k 0 under the CAT's
/// wrapper, created by the CAT settlement coin, and its change at k 1 with
/// label 0, from its own spend group's tweak point; and the CAT payment is
/// spent with the one-time key.
fn check_maker_paid_silently(block: &mut OfferBlock) -> Result<()> {
    let w = block.w;
    let p = block.maker_p;
    let maker_coin_ids: Vec<Bytes32> = block.maker_coins.iter().map(|c| c.coin.coin_id()).collect();

    // The published request: one notarized payment for the CAT, under the
    // maker's nonce, listing (p_0 1000 (p_0)); the asset info has the hidden
    // puzzle hash.
    let requests = &block.maker_outputs.silent_payment_requests;
    assert!(requests.xch.is_empty());
    assert_eq!(requests.cats.len(), 1);
    let notarized = &requests.cats[&w.asset_id];
    assert_eq!(notarized.len(), 1);
    let notarized = &notarized[0];
    assert_eq!(
        notarized.nonce,
        Offer::nonce(maker_coin_ids.clone()),
        "the maker's nonce"
    );
    assert_eq!(notarized.payments.len(), 1);
    assert_eq!(notarized.payments[0].puzzle_hash, p[0]);
    assert_eq!(notarized.payments[0].amount, 1_000);
    let memos = match notarized.payments[0].memos {
        Memos::Some(ptr) => block.ctx.extract::<Vec<Bytes>>(ptr)?,
        Memos::None => panic!("a CAT payment carries its memo list"),
    };
    assert_eq!(memos, hint_of(p[0]), "(p_0 1000 (p_0))");
    assert_eq!(
        block
            .maker_outputs
            .silent_payment_asset_info
            .cat(w.asset_id)
            .map(|info| info.hidden_puzzle_hash),
        Some(w.hidden_puzzle_hash)
    );

    // Exactly one puzzle announcement is asserted by the maker's coins: the
    // CAT settlement coin's announcement of that notarized payment.
    let announcement = settlement_announcement(&block.ctx, Some(&w), notarized);
    assert_eq!(
        puzzle_announcement_assertions(&block.maker_coin_spends, &maker_coin_ids)?,
        vec![announcement]
    );
    // The two maker coins are one cycle, and the change is created by one of
    // them.
    for &coin_id in &maker_coin_ids {
        assert_eq!(
            concurrent_spend_assertions(&block.maker_coin_spends, coin_id)?,
            1
        );
    }
    assert!(
        block
            .maker_outputs
            .xch
            .iter()
            .any(|c| c.puzzle_hash == p[1] && c.amount == 200),
        "the change at p_1"
    );

    // The maker's scan of the block.
    let (tweak_data, detections) = scan_block(&block.sim, block.height, &block.maker);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(detections.len(), 2, "the CAT payment and the change");
    let payment = detections
        .iter()
        .find(|d| d.k == 0)
        .expect("the CAT payment at k 0");
    assert_eq!(payment.label, None);
    assert_eq!(payment.wrapper, Some(w));
    assert_eq!(payment.puzzle_hash, p[0]);
    assert_eq!(payment.coin().puzzle_hash, w.wrap(p[0]));
    assert_eq!(payment.amount, 1_000);
    // Its parent is a CAT settlement coin spent in the block.
    let parent = block
        .sim
        .block_spends(block.height)
        .into_iter()
        .find(|cs| cs.coin.coin_id() == payment.parent_coin_id)
        .expect("the parent is spent in the block");
    assert_eq!(parent.coin.puzzle_hash, settlement_puzzle_hash(Some(&w)));
    assert_eq!(payment.coin(), block.maker_paid.coin);

    let change = detections
        .iter()
        .find(|d| d.k == 1)
        .expect("the change at k 1");
    assert_eq!(change.label, Some(0));
    assert_eq!(change.wrapper, None);
    assert_eq!(change.puzzle_hash, p[1]);
    assert_eq!(change.amount, 200);
    assert!(maker_coin_ids.contains(&change.parent_coin_id));

    // Both are found from the tweak point of the maker's own spend group.
    let a_sum = aggregate_sender_sks(&[
        block.maker_coins[0].sk.clone(),
        block.maker_coins[1].sk.clone(),
    ])?;
    let own = compute_tweak_point(&maker_coin_ids, &a_sum.public_key()).expect("tweak point");
    assert_eq!(
        scan_one_point(&block.maker, own, &tweak_data).len(),
        2,
        "from the maker's own group"
    );

    // Without the wrapper list the CAT payment at k 0 is missed, and the scan
    // stops there.
    let bare = chia_sdk_driver::silent_payments::TweakData::new(
        tweak_data.tweak_points.clone(),
        tweak_data.outputs.clone(),
    );
    assert!(
        scan_from_tweaks(
            block.maker.scan_sk(),
            block.maker.spend_pk(),
            &bare,
            None,
            K_MAX_DEFAULT,
        )
        .is_empty()
    );

    // The CAT payment is spent with the one-time key.
    let destination = block.taker.puzzle_hash;
    let paid = block.maker_paid;
    let maker = block.maker.clone();
    let detected = payment.clone();
    spend_detected_cat(
        &mut block.sim,
        &mut block.ctx,
        paid,
        &detected,
        &maker,
        destination,
    )
}

/// Scans `tweak_data`'s outputs and wrapper list with the one tweak point
/// `point` only.
fn scan_one_point(
    keys: &SilentPaymentKeys,
    point: chia_bls::PublicKey,
    tweak_data: &chia_sdk_driver::silent_payments::TweakData,
) -> Vec<chia_sdk_driver::silent_payments::DetectedSpCoin> {
    let one = chia_sdk_driver::silent_payments::TweakData::with_wrappers(
        vec![point],
        tweak_data.outputs.clone(),
        tweak_data.wrappers.clone(),
    );
    scan_from_tweaks(keys.scan_sk(), keys.spend_pk(), &one, None, K_MAX_DEFAULT)
}

/// The notarized payments in the solution of the settlement coin `coin_id`
/// among `coin_spends`, each as its nonce and tree hash.
fn settlement_solution(
    coin_spends: &[CoinSpend],
    coin_id: Bytes32,
) -> Result<Vec<(Bytes32, Bytes32)>> {
    let spend = coin_spends
        .iter()
        .find(|cs| cs.coin.coin_id() == coin_id)
        .expect("the settlement coin is spent");
    let mut a = Allocator::new();
    let solution = spend.solution.to_clvm(&mut a)?;
    let solution = SettlementPaymentsSolution::from_clvm(&a, solution)?;
    Ok(solution
        .notarized_payments
        .iter()
        .map(|np| (np.nonce, Bytes32::from(tree_hash_notarized_payment(&a, np))))
        .collect())
}

/// Checks the taker's side of an offer block in which the taker is paid
/// silently: the offered XCH settlement coin's solution holds one notarized
/// payment for the taker under its nonce, listing `(p_0 amount ())` (and the
/// other payment after it); the taker's CAT coin asserts its announcement
/// once; the taker's scan finds the XCH payment at k 0 bare, created by the
/// settlement coin, and its CAT change at k 1 with label 0 under the CAT's
/// wrapper, from its own spend group's tweak point; and both are spent with the
/// one-time keys.
fn check_taker_paid_silently(block: &mut OfferBlock, xch_amount: u64) -> Result<()> {
    let w = block.w;
    let p = block.taker_p;
    let id = Id::Existing(w.asset_id);

    // The taker's notarized payment in XCH, as the CHIP encodes it.
    let mut payments = vec![Payment::new(
        p[0],
        xch_amount,
        Memos::Some(clvmr::NodePtr::NIL),
    )];
    if xch_amount < 1_000 {
        let other = block
            .taker_outputs
            .xch
            .iter()
            .find(|c| c.parent_coin_info == block.offered_xch.coin_id() && c.puzzle_hash != p[0])
            .expect("the other payment");
        payments.push(Payment::new(
            other.puzzle_hash,
            other.amount,
            Memos::Some(clvmr::NodePtr::NIL),
        ));
    }
    let notarized = NotarizedPayment::new(block.taker_nonce, payments);
    let tree_hash = Bytes32::from(tree_hash_notarized_payment(&block.ctx, &notarized));
    assert_eq!(
        settlement_solution(&block.taker_coin_spends, block.offered_xch.coin_id())?,
        vec![(block.taker_nonce, tree_hash)],
        "one notarized payment for the taker, under its nonce"
    );

    // The taker's coin asserts its announcement once.
    let announcement = settlement_announcement(&block.ctx, None, &notarized);
    let asserted = puzzle_announcement_assertions(
        &block.taker_coin_spends,
        &[block.taker_cat.coin.coin_id()],
    )?;
    assert_eq!(
        asserted.iter().filter(|&&a| a == announcement).count(),
        1,
        "the taker's group asserts its announcement once"
    );
    assert!(block.taker_outputs.xch.contains(&Coin::new(
        block.offered_xch.coin_id(),
        p[0],
        xch_amount
    )));

    // The taker's scan of the block.
    let (tweak_data, detections) = scan_block(&block.sim, block.height, &block.taker_keys);
    assert_eq!(tweak_data.wrappers, vec![w]);
    assert_eq!(detections.len(), 2, "the XCH payment and the CAT change");
    let payment = detections
        .iter()
        .find(|d| d.k == 0)
        .expect("the XCH payment at k 0");
    assert_eq!(payment.label, None);
    assert_eq!(payment.wrapper, None);
    assert_eq!(payment.puzzle_hash, p[0]);
    assert_eq!(payment.amount, xch_amount);
    assert_eq!(payment.parent_coin_id, block.offered_xch.coin_id());
    let change = detections
        .iter()
        .find(|d| d.k == 1)
        .expect("the CAT change at k 1");
    assert_eq!(change.label, Some(0));
    assert_eq!(change.wrapper, Some(w));
    assert_eq!(change.puzzle_hash, p[1]);
    assert_eq!(change.amount, 500);
    assert_eq!(change.parent_coin_id, block.taker_cat.coin.coin_id());

    // Both from the tweak point of the taker's own group, its one CAT coin.
    let own = compute_tweak_point(&[block.taker_cat.coin.coin_id()], &block.taker.pk)
        .expect("tweak point");
    assert_eq!(
        scan_one_point(&block.taker_keys, own, &tweak_data).len(),
        2,
        "from the taker's own group"
    );

    // Both are spent with their one-time keys.
    let synthetic_secret = payment
        .onetime_sk(block.taker_keys.spend_sk())
        .derive_synthetic();
    StandardLayer::new(synthetic_secret.public_key()).spend(
        &mut block.ctx,
        payment.coin(),
        Conditions::new().create_coin(block.taker.puzzle_hash, xch_amount, Memos::None),
    )?;
    block
        .sim
        .spend_coins(block.ctx.take(), std::slice::from_ref(&synthetic_secret))?;
    let paid_change = *block.taker_outputs.cats[&id]
        .iter()
        .find(|c| c.info.p2_puzzle_hash == p[1])
        .expect("the CAT change");
    let keys = block.taker_keys.clone();
    let destination = block.taker.puzzle_hash;
    let detected = change.clone();
    spend_detected_cat(
        &mut block.sim,
        &mut block.ctx,
        paid_change,
        &detected,
        &keys,
        destination,
    )
}

/// Plan 15-03 tracer, CHIP-0057 "Maker paid silently": the maker offers XCH
/// from two coins bound in a cycle and requests a CAT at its own silent
/// payment address, with its XCH change at label 0. A taker takes the offer
/// with the SDK's ordinary flow, and the maker's scan finds the CAT payment at
/// k 0 under the CAT's wrapper and its change at k 1 with label 0.
#[test]
fn test_maker_paid_silently_through_a_settlement_coin() -> Result<()> {
    let mut block = offer_block(OfferCase {
        maker_silent: true,
        taker_silent: false,
        hidden: None,
        taker_other: None,
    })?;
    check_maker_paid_silently(&mut block)
}

/// CHIP-0057 "Taker paid silently": the taker gives a CAT from one coin and is
/// paid the offered XCH at k 0 through the XCH settlement coin, with
/// `Action::silent_payment_settle`; its CAT change is at k 1 with label 0.
#[test]
fn test_taker_paid_silently_through_a_settlement_coin() -> Result<()> {
    let mut block = offer_block(OfferCase {
        maker_silent: false,
        taker_silent: true,
        hidden: None,
        taker_other: None,
    })?;
    // The maker, paid at its own puzzle hash, is paid by the CAT settlement
    // coin as usual.
    assert!(
        block
            .sim
            .coin_state(block.maker_paid.coin.coin_id())
            .is_some()
    );
    check_taker_paid_silently(&mut block, 1_000)
}

/// CHIP-0057 "Both parties": both are paid silently in one block. Each derives
/// from its own spend group, finds its own payments from its own group's tweak
/// point, and nothing from the other's.
#[test]
fn test_both_parties_paid_silently() -> Result<()> {
    let mut block = offer_block(OfferCase {
        maker_silent: true,
        taker_silent: true,
        hidden: None,
        taker_other: None,
    })?;
    let (tweak_data, _) = scan_block(&block.sim, block.height, &block.maker);
    let maker_coin_ids: Vec<Bytes32> = block.maker_coins.iter().map(|c| c.coin.coin_id()).collect();
    let a_sum = aggregate_sender_sks(&[
        block.maker_coins[0].sk.clone(),
        block.maker_coins[1].sk.clone(),
    ])?;
    let maker_point =
        compute_tweak_point(&maker_coin_ids, &a_sum.public_key()).expect("tweak point");
    let taker_point = compute_tweak_point(&[block.taker_cat.coin.coin_id()], &block.taker.pk)
        .expect("tweak point");
    assert!(scan_one_point(&block.maker, taker_point, &tweak_data).is_empty());
    assert!(scan_one_point(&block.taker_keys, maker_point, &tweak_data).is_empty());

    check_maker_paid_silently(&mut block)?;
    check_taker_paid_silently(&mut block, 1_000)
}

/// CHIP-0057 Required Behaviors, Offers: a party with two requested payments
/// in one asset, one of them at an address that is not a silent payment
/// address, asserts one puzzle announcement over the notarized payment that
/// lists both, the silent payment first (Test Vector 13's
/// `two_payments_in_one_asset`), even when the other payment's action comes
/// first. Both coins are created by the settlement coin.
#[test]
fn test_other_payment_in_the_settled_asset_shares_one_announcement() -> Result<()> {
    let q = Bytes32::new([0x71; 32]);
    let mut block = offer_block(OfferCase {
        maker_silent: false,
        taker_silent: true,
        hidden: None,
        taker_other: Some((q, 250)),
    })?;
    let other = Coin::new(block.offered_xch.coin_id(), q, 250);
    assert!(block.taker_outputs.xch.contains(&other));
    assert!(block.sim.coin_state(other.coin_id()).is_some());
    check_taker_paid_silently(&mut block, 750)
}

/// CHIP-0057 "The rule covers a settlement coin of any asset W, including a
/// revocable CAT" (RESEARCH Pitfall 8): the maker requests a revocable CAT
/// silently. `Outputs::silent_payment_asset_info` carries the hidden puzzle
/// hash, so the offer file lists the revocable settlement coin the maker's
/// assertion names, the offer settles, and the payment is found under
/// `Wrapper::revocable`.
#[test]
fn test_maker_requests_a_revocable_cat_silently() -> Result<()> {
    let hidden = Bytes32::new([7; 32]);
    let mut block = offer_block(OfferCase {
        maker_silent: true,
        taker_silent: false,
        hidden: Some(hidden),
        taker_other: None,
    })?;
    let w = Wrapper::revocable(block.w.asset_id, hidden);
    assert_eq!(block.w, w);
    // The offer file's requested CAT payment is made by the revocable CAT's
    // settlement coin.
    assert!(
        block
            .offer_file
            .coin_spends
            .iter()
            .any(|cs| cs.coin.parent_coin_info == Bytes32::default()
                && cs.coin.puzzle_hash == settlement_puzzle_hash(Some(&w)))
    );
    assert_ne!(
        settlement_puzzle_hash(Some(&w)),
        settlement_puzzle_hash(Some(&Wrapper::cat(w.asset_id)))
    );
    check_maker_paid_silently(&mut block)
}

/// `Action::silent_payment_settle` needs a settlement coin of the payment's
/// asset in the transaction (the taker's case): without one, or with only a
/// settlement coin of a CAT without the requested hidden puzzle hash, it fails
/// with `SilentPaymentSettlementCoinMissing`.
#[test]
fn test_settle_without_a_settlement_coin_fails() -> Result<()> {
    let (mut sim, mut ctx, sender, recipient) = setup_e2e()?;
    let _ = &mut sim;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let nonce = Bytes32::new([0x55; 32]);
    let asset_id = Bytes32::new([0x33; 32]);

    // Only an XCH coin of the standard puzzle.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    for asset in [None, Some(Wrapper::cat(asset_id))] {
        let result = spends.apply(
            &mut ctx,
            &[Action::silent_payment_settle(
                asset,
                address.clone(),
                100,
                nonce,
            )],
        );
        assert!(
            matches!(result, Err(DriverError::SilentPaymentSettlementCoinMissing)),
            "expected SilentPaymentSettlementCoinMissing, got {result:?}"
        );
    }

    // A settlement coin of the CAT that is not revocable does not settle the
    // revocable CAT; it does settle the plain one.
    let info = chia_sdk_driver::CatInfo::new(asset_id, None, SETTLEMENT_PAYMENT_HASH.into());
    let settlement = Cat::new(
        Coin::new(Bytes32::new([0x11; 32]), info.puzzle_hash().into(), 1_000),
        None,
        info,
    );
    spends.add(settlement);
    let result = spends.apply(
        &mut ctx,
        &[Action::silent_payment_settle(
            Some(Wrapper::revocable(asset_id, Bytes32::new([7; 32]))),
            address.clone(),
            100,
            nonce,
        )],
    );
    assert!(
        matches!(result, Err(DriverError::SilentPaymentSettlementCoinMissing)),
        "expected SilentPaymentSettlementCoinMissing, got {result:?}"
    );
    spends.apply(
        &mut ctx,
        &[Action::silent_payment_settle(
            Some(Wrapper::cat(asset_id)),
            address,
            100,
            nonce,
        )],
    )?;
    Ok(())
}

/// A party's settled entries are one notarized payment per asset under one
/// nonce, with one payer per asset: entries with two nonces, or an asset paid
/// both by the counterparty's settlement coin and by one the transaction
/// spends, fail with `SilentPaymentSettlementMismatch`. Nothing is emitted.
#[test]
fn test_settled_entries_with_two_nonces_fail() -> Result<()> {
    let (_sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let w = Wrapper::cat(Bytes32::new([0x33; 32]));
    let nonce = Bytes32::new([0x55; 32]);
    let other_nonce = Bytes32::new([0x56; 32]);
    let settlement_coin = Coin::new(
        Bytes32::new([0x11; 32]),
        SETTLEMENT_PAYMENT_HASH.into(),
        1_000,
    );
    let other_payment = Payment::new(
        Bytes32::new([0x71; 32]),
        10,
        Memos::Some(clvmr::NodePtr::NIL),
    );

    let cases: Vec<(Vec<Action>, &str)> = vec![
        (
            vec![
                Action::silent_payment_requested(Some(w), address.clone(), 100, nonce),
                Action::silent_payment_requested(None, address.clone(), 100, other_nonce),
            ],
            "mismatch",
        ),
        (
            vec![
                Action::silent_payment_requested(None, address.clone(), 100, nonce),
                Action::silent_payment_settle(None, address.clone(), 100, nonce),
            ],
            "mismatch",
        ),
        (
            vec![
                Action::silent_payment_requested(None, address.clone(), 100, nonce),
                Action::silent_payment_requested_other(None, other_payment, other_nonce),
            ],
            "mismatch",
        ),
    ];
    for (actions, expected) in cases {
        let mut spends = Spends::new(sender.puzzle_hash);
        spends.add(sender.coin);
        spends.add(settlement_coin);
        let deltas = spends.apply(&mut ctx, &actions)?;
        let pk_map = register_keys(&mut spends, &[&sender]);
        let result =
            spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
        assert_eq!(expected, "mismatch");
        assert!(
            matches!(result, Err(DriverError::SilentPaymentSettlementMismatch)),
            "expected SilentPaymentSettlementMismatch, got {result:?}"
        );
        assert!(ctx.take().is_empty(), "nothing is emitted");
    }
    Ok(())
}

/// CHIP-0057 `DeriveSettlementOutputs`: an asset with other payments only
/// gets its own notarized payment and assertion, after the assets of the
/// silent payments. The action system and the pure `derive_settlement_outputs`
/// agree byte for byte on the same inputs: the notarized payments the maker
/// publishes are the derivation's, in its order, and the announcements the
/// maker's coin asserts are the derivation's assertions.
#[test]
fn test_settlement_paths_agree_on_an_asset_with_other_payments_only() -> Result<()> {
    let (_sim, mut ctx, sender, recipient) = setup_e2e()?;
    let address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);
    let w = Wrapper::cat(Bytes32::new([0x33; 32]));
    let nonce = Bytes32::new([0x55; 32]);
    let other_payment = Payment::new(
        Bytes32::new([0x71; 32]),
        10,
        Memos::Some(clvmr::NodePtr::NIL),
    );

    // The action system: an XCH silent payment and a CAT other payment, both
    // requested from the counterparty.
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::silent_payment_requested(None, address.clone(), 100, nonce),
            Action::silent_payment_requested_other(Some(w), other_payment.clone(), nonce),
        ],
    )?;
    let pk_map = register_keys(&mut spends, &[&sender]);
    let outputs =
        spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;
    let coin_spends = ctx.take();
    let asserted = puzzle_announcement_assertions(&coin_spends, &[sender.coin.coin_id()])?;

    // The pure function, over the same group, entries, other payment and nonce.
    let derivation = derive_settlement_outputs(
        &mut ctx,
        &[SettlementGroupCoin::new(
            &sender.coin,
            SyntheticSecretKey::from_synthetic_unchecked(sender.sk.clone()),
            None,
        )],
        &[SettlementRecipient {
            scan_pk: address.scan_pk,
            spend_pk: address.spend_pk,
            wrapper: None,
            settled: true,
            amount: 100,
        }],
        &[OtherPayment {
            wrapper: Some(w),
            payment: other_payment,
        }],
        nonce,
    )?;
    assert_eq!(derivation.notarized_payments.len(), 2);
    assert_eq!(derivation.notarized_payments[0].wrapper, None);
    assert_eq!(derivation.notarized_payments[1].wrapper, Some(w));

    let requests = &outputs.silent_payment_requests;
    assert_eq!(requests.xch.len(), 1);
    let cat_requests = requests.cats.get(&w.asset_id).expect("the CAT's request");
    assert_eq!(cat_requests.len(), 1);
    let bytes_of = |ctx: &mut SpendContext, np: &NotarizedPayment| -> Result<Vec<u8>> {
        let ptr = ctx.alloc(np)?;
        Ok(clvmr::serde::node_to_bytes(ctx, ptr)?)
    };
    for (published, derived) in [&requests.xch[0], &cat_requests[0]]
        .into_iter()
        .zip(&derivation.notarized_payments)
    {
        assert_eq!(
            bytes_of(&mut ctx, published)?,
            bytes_of(&mut ctx, &derived.notarized_payment)?
        );
    }
    let derived: Vec<Bytes32> = derivation
        .assertions
        .iter()
        .map(|a| a.announcement_id)
        .collect();
    assert_eq!(asserted, derived);
    Ok(())
}

/// Plan 15-03 Task 3, CHIP-0057 Appendix A and Required Behaviors,
/// Compatibility: in the block of a maker-silent offer, the maker's spend group
/// (its two XCH coins) has among its outputs the CAT settlement coin's child
/// `wrap(W, p_0)`, through the settlement coin's puzzle announcement the maker
/// asserts. A scan of the group's tweak point against its outputs finds the
/// payment and the change; against the children of its own coins only, it
/// finds nothing (the change at k 1 is not reached once k 0 is missed).
#[test]
fn test_maker_group_outputs_include_the_settlement_child() -> Result<()> {
    let block = offer_block(OfferCase {
        maker_silent: true,
        taker_silent: false,
        hidden: None,
        taker_other: None,
    })?;
    let spends = block.sim.block_spends(block.height);
    let additions = block.sim.block_outputs(block.height);
    let groups = spend_groups_from_block_spends(&spends, &additions)?;

    let mut maker_coin_ids: Vec<Bytes32> =
        block.maker_coins.iter().map(|c| c.coin.coin_id()).collect();
    let group = groups
        .iter()
        .find(|g| {
            let mut ids = g.coin_ids.clone();
            ids.sort();
            maker_coin_ids.sort();
            ids == maker_coin_ids
        })
        .expect("the maker's spend group");
    assert_eq!(group.member_wrappers, vec![None, None]);
    let settlement_child = block.maker_paid.coin;
    assert_eq!(settlement_child.puzzle_hash, block.w.wrap(block.maker_p[0]));
    assert!(
        group.outputs.contains(&settlement_child),
        "the settlement child is an output of the maker's group"
    );
    let members_only: Vec<Coin> = additions
        .iter()
        .filter(|c| maker_coin_ids.contains(&c.parent_coin_info))
        .copied()
        .collect();
    assert!(!members_only.contains(&settlement_child));
    for coin in &members_only {
        assert!(group.outputs.contains(coin), "members' children first");
    }

    // The tweak data's tweak points are the groups', and its outputs are still
    // every addition.
    let tweak_data = tweak_data_from_block_spends(&spends, &additions)?;
    assert!(tweak_data.tweak_points.contains(&group.tweak_point));
    assert_eq!(tweak_data.outputs.len(), additions.len());

    let scan = |outputs: &[Coin]| {
        let data = chia_sdk_driver::silent_payments::TweakData::with_wrappers(
            vec![group.tweak_point],
            outputs
                .iter()
                .map(|coin| OutputMeta {
                    puzzle_hash: coin.puzzle_hash,
                    coin_id: coin.coin_id(),
                    amount: coin.amount,
                    parent_coin_id: coin.parent_coin_info,
                })
                .collect(),
            tweak_data.wrappers.clone(),
        );
        scan_from_tweaks(
            block.maker.scan_sk(),
            block.maker.spend_pk(),
            &data,
            None,
            K_MAX_DEFAULT,
        )
    };
    let found = scan(&group.outputs);
    assert_eq!(found.len(), 2, "the CAT payment and the change");
    assert!(
        found
            .iter()
            .any(|d| d.k == 0 && d.coin() == settlement_child && d.wrapper == Some(block.w))
    );
    assert!(scan(&members_only).is_empty(), "members' children only");
    Ok(())
}
