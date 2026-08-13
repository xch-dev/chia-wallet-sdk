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
use chia_protocol::Coin;
use chia_puzzle_types::{DeriveSynthetic, Memos};
use chia_sdk_driver::silent_payments::{
    K_MAX_DEFAULT, SyntheticPublicKey, SyntheticSecretKey, scan_from_tweaks,
};
use chia_sdk_driver::{Action, DriverError, Relation, SpendContext, Spends, StandardLayer};
use chia_sdk_test::silent_payments::tweak_data_from_simulator_block;
use chia_sdk_test::{BlsPairWithCoin, Simulator};
use chia_sdk_types::Conditions;
use chia_sdk_utils::silent_payments::{LabelRegistry, SilentPaymentKeys, SilentPaymentNetwork};
use indexmap::indexmap;

/// Stable BIP-39 test-vector mnemonic — matches the cross-language AVA fixture
/// for deterministic seeds.
const TV1_MNEMONIC: &str =
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
    let mnemonic = Mnemonic::parse(TV1_MNEMONIC)?;
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
///    applying [`DeriveSynthetic::derive_synthetic`] to the `onetime_sk`.
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
        recipient.spend_sk(),
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

    // Spend the detected coin: derive_synthetic() then StandardLayer.
    let synthetic_secret = detected.onetime_sk.derive_synthetic();
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

/// Multi-input unlabeled SP send round-trip: TWO distinct-puzzle-hash XCH
/// inputs sent to ONE unlabeled SP address with `Relation::AssertConcurrent`.
///
/// This drives the real receiver grouping path: the sender aggregates both
/// inputs' synthetic SKs, the `AssertConcurrent` cycle binds the two coins into
/// one strongly connected component, and the receiver reconstructs the same
/// `input_hash` over the same input set.
///
/// Asserts:
/// 1. The `AssertConcurrent` runtime gate is satisfied (2 non-ephemeral XCH
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

    let mnemonic = Mnemonic::parse(TV1_MNEMONIC)?;
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

    // MUST be AssertConcurrent for 2+ non-ephemeral XCH inputs — otherwise the
    // runtime gate errors with SilentPaymentRequiresInputBinding.
    spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;

    // Farm: sign with BOTH senders' SKs.
    sim.spend_coins(ctx.take(), &[a.sk.clone(), b.sk.clone()])?;

    // Extract + scan via the canonical helper + free-fn scanner.
    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_sk(),
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
    let synthetic_secret = detected.onetime_sk.derive_synthetic();
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

/// Mixed-asset SP bundles are REJECTED: a silent-payment send co-bundled with a
/// CAT issuance (a non-XCH asset spend) in the same bundle must hard-error.
/// Enforces the XCH-only invariant — silent-payment send bundles must not
/// co-spend CAT/DID/NFT/option coins. `finish_with_keys` returns
/// `Err(DriverError::SilentPaymentMixedAssetBundle)` from the GATE-0 guard in
/// `sp_finish_branch`, which fires before any derivation or key work.
#[test]
fn test_simulator_e2e_multi_input_mixed_asset_rejected() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();
    let a = sim.bls(600);
    let b = sim.bls(600);
    let mnemonic = Mnemonic::parse(TV1_MNEMONIC)?;
    let recipient = SilentPaymentKeys::from_mnemonic(&mnemonic);
    let recipient_address = recipient.unlabeled_address(SilentPaymentNetwork::Testnet);

    let mut spends = Spends::new(a.puzzle_hash);
    spends.add(a.coin);
    spends.add(b.coin);

    // Co-bundle a CAT issuance (non-XCH asset) with the SP send.
    let deltas = spends.apply(
        &mut ctx,
        &[
            Action::single_issue_cat(None, 1),
            Action::silent_payment_send(recipient_address, 1000, Memos::None),
        ],
    )?;

    let pk_map = indexmap! { a.puzzle_hash => a.pk, b.puzzle_hash => b.pk };
    let synthetic_public_map = indexmap! {
        a.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(a.pk),
        b.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(b.pk),
    };
    let synthetic_secret_map = indexmap! {
        a.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(a.sk.clone()),
        b.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(b.sk.clone()),
    };
    spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

    let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);
    assert!(
        matches!(result, Err(DriverError::SilentPaymentMixedAssetBundle)),
        "mixed-asset SP bundle must be rejected, got {result:?}"
    );
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
        recipient.spend_sk(),
        recipient.spend_pk(),
        &tweak_data,
        Some(&labels),
        K_MAX_DEFAULT,
    );
    assert_eq!(detections.len(), 1, "expected exactly 1 labeled detection");
    let detected = &detections[0];
    assert_eq!(detected.label, Some(1), "labeled at m=1");
    assert_eq!(detected.amount, 200);

    // Follow-on spend (labeled): the scanner already absorbed `label_scalar`
    // into `onetime_sk`, so `derive_synthetic()` works identically to the
    // unlabeled case (per the scanner's `labeled_sk = base_sk + label_scalar`).
    let synthetic_secret = detected.onetime_sk.derive_synthetic();
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

/// `m=0` self-change consistency: the SDK does NOT auto-emit `m=0` self-change
/// outputs. This test documents the actual contract:
///
/// 1. `LabelRegistry::register(scan_sk, 0)` is callable internally (per
///    `chia-sdk-utils/src/silent_payments/labels.rs`).
/// 2. A self-send to one's own UNLABELED address detects normally with
///    `label: None` — the `m=0` registry entry does NOT promote the detection.
/// 3. The labeled detection branch in `scan_from_tweaks` only runs when no
///    unlabeled match is found at the current `k` (per the scanner's
///    `if !found` ordering) — so registering `m=0` cannot spuriously hijack
///    unlabeled detections.
///
/// `labeled_address(0)` returns `Err(ReservedChangeLabel)` at the public
/// boundary: `m=0` is reserved for wallet-author-managed internal change
/// tracking, not a public address shape.
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

    // Register m=0 in the recipient's LabelRegistry — internal-only API path.
    // This MUST be possible AND MUST NOT corrupt unlabeled detection of the
    // self-send below.
    let mut labels = LabelRegistry::new();
    labels.register(recipient.scan_sk(), 0);

    let tweak_data = tweak_data_from_simulator_block(&sim, height_before);
    let detections = scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_sk(),
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
