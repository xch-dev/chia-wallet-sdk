//! CHIP-0057 silent-payments end-to-end demo.
//!
//! Mirrors `examples/cat_spends.rs`: simulator setup → sender BLS pair →
//! recipient SP keys (BIP-39 mnemonic) → two SP sends in one tx (unlabeled +
//! labeled m=1) → farm → extract `TweakData` via `tweak_data_from_simulator_block`
//! → scan → detect both outputs → spend each via `StandardLayer` after
//! `.derive_synthetic()` (Stages 1-5), plus a multi-input section (Stages
//! 6-9) demonstrating the `tweak_data_from_block_spends` helper over the
//! simulator's block accessors with `Relation::AssertConcurrent` cycle
//! binding for two non-ephemeral XCH inputs.
//!
//! Tweak-data extraction uses only the test-crate helper — no transport
//! client is referenced (forward-compat). The labeled address uses m=1;
//! `labeled_address(0)` errors with `ReservedChangeLabel`.
//!
//! Run: `cargo run --example silent_payment --all-features`

use anyhow::Result;
use bip39::Mnemonic;
use chia_puzzle_types::DeriveSynthetic;
use chia_wallet_sdk::prelude::*;
use indexmap::indexmap;

/// BIP-39 TV1 mnemonic — stable, well-known, deterministic across runs.
/// Matches the fixture used by the CHIP-0057 test vectors and the SDK's
/// silent-payments unit + binding tests.
const TV1_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn main() -> Result<()> {
    // 1. Setup: simulator, spend context, sender BLS pair, recipient SP keys.
    let mut sim = Simulator::new();
    let ctx = &mut SpendContext::new();
    let sender = sim.bls(1_000);
    let mnemonic = Mnemonic::parse(TV1_MNEMONIC)?;
    let recipient = SilentPaymentKeys::from_mnemonic(&mnemonic);

    // Derive both an unlabeled and a labeled (m=1) address from the same keys.
    let unlabeled_addr = recipient.unlabeled_address(SilentPaymentNetwork::Mainnet);
    let labeled_addr = recipient.labeled_address(SilentPaymentNetwork::Mainnet, 1)?;

    println!("Stage 1/5 — Addresses:");
    println!("  unlabeled:  {}", unlabeled_addr.encode()?);
    println!("  labeled(1): {}", labeled_addr.encode()?);

    // 2. Send 100 mojos to the unlabeled address and 200 mojos to labeled(m=1)
    //    in one tx via Action::silent_payment_send.
    let height_before = sim.height();
    let mut spends = Spends::new(sender.puzzle_hash);
    spends.add(sender.coin);
    let deltas = spends.apply(
        ctx,
        &[
            Action::silent_payment_send(unlabeled_addr, 100, Memos::None),
            Action::silent_payment_send(labeled_addr, 200, Memos::None),
        ],
    )?;
    // `pks` stays raw for `finish_with_keys` (used to spend the coin). The SP
    // key maps wrap synthetic keys via the synthetic-key newtypes — here the
    // `sim.bls()` coin is curried over the raw pk, so the registered key IS the
    // raw key and we wrap via `from_synthetic_unchecked`. Wallets sending from
    // standard-spend coins synthesize via `SyntheticSecretKey::from_raw`.
    let pks = indexmap! { sender.puzzle_hash => sender.pk };
    spends.with_silent_payment_keys(
        indexmap! { sender.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender.pk) },
        indexmap! {
            sender.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender.sk.clone()),
        },
    );
    spends.finish_with_keys(ctx, &deltas, Relation::None, &pks)?;
    sim.spend_coins(ctx.take(), std::slice::from_ref(&sender.sk))?;
    println!("Stage 2/5 — Sent 100 mojos unlabeled + 200 mojos labeled(m=1) in one tx.");

    // 3. Extract: walk the just-farmed block to build TweakData via the test-crate
    //    helper directly. Fully-qualified path because the umbrella prelude does
    //    not re-export it (forward-compat: no transport client is referenced).
    let tweak_data =
        chia_sdk_test::silent_payments::tweak_data_from_simulator_block(&sim, height_before);
    println!(
        "Stage 3/5 — Extracted TweakData: {} tweak_point(s), {} output(s).",
        tweak_data.tweak_points.len(),
        tweak_data.outputs.len(),
    );

    // 4. Scan: register m=1 in a fresh LabelRegistry so the scanner can attribute
    //    the labeled output. Unlabeled detection always works regardless of registry.
    let mut labels = LabelRegistry::new();
    labels.register(recipient.scan_sk(), 1);
    let detections = recipient.scan(&tweak_data, Some(&labels), K_MAX_DEFAULT);
    println!(
        "Stage 4/5 — Scanned: detected {} output(s).",
        detections.len()
    );
    for d in &detections {
        println!(
            "  coin_id={} amount={} label={:?} k={}",
            d.coin_id, d.amount, d.label, d.k,
        );
    }

    // 5. Spend: for each detected coin, derive the synthetic secret key from
    //    onetime_sk (mandatory — the puzzle currys StandardArgs(synthetic_key),
    //    so signing with the raw onetime_sk would produce an invalid signature),
    //    then spend via StandardLayer leaving (amount - 1) and a 1-mojo fee.
    for d in &detections {
        let synthetic_secret = d.onetime_sk.derive_synthetic();
        let conditions = Conditions::new()
            .create_coin(sender.puzzle_hash, d.amount - 1, Memos::None)
            .reserve_fee(1);
        let coin = Coin::new(d.parent_coin_id, d.puzzle_hash, d.amount);
        StandardLayer::new(synthetic_secret.public_key()).spend(ctx, coin, conditions)?;
        sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;
        println!(
            "Stage 5/5 — Spent detected coin {} (label={:?}).",
            d.coin_id, d.label
        );
    }

    // ─── Multi-input SP send demo ────────────────────────────────────────
    // Same recipient, two sender coins bound by Relation::AssertConcurrent.
    // Demonstrates the canonical multi-input flow that downstream wallets
    // use when assembling multi-coin SP sends — the receive-side scanner
    // re-groups the inputs via Pass 2b SCC over opcode-64
    // AssertConcurrentSpend edges so a single TweakData tweak_point
    // emerges for the multi-input transaction.

    let sender_a = sim.bls(500);
    let sender_b = sim.bls(500);
    let height_before_multi = sim.height();

    let mut spends_multi = Spends::new(sender_a.puzzle_hash);
    spends_multi.add(sender_a.coin);
    spends_multi.add(sender_b.coin);

    let multi_addr = recipient.unlabeled_address(SilentPaymentNetwork::Mainnet);
    let deltas_multi = spends_multi.apply(
        ctx,
        &[Action::silent_payment_send(multi_addr, 700, Memos::None)],
    )?;

    // `multi_pks` stays raw for `finish_with_keys`; the SP key maps wrap the
    // raw fixture keys via `from_synthetic_unchecked` (coins curried over the
    // raw pk).
    let multi_pks = indexmap! {
        sender_a.puzzle_hash => sender_a.pk,
        sender_b.puzzle_hash => sender_b.pk,
    };
    spends_multi.with_silent_payment_keys(
        indexmap! {
            sender_a.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender_a.pk),
            sender_b.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(sender_b.pk),
        },
        indexmap! {
            sender_a.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender_a.sk.clone()),
            sender_b.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(sender_b.sk.clone()),
        },
    );
    // Relation::AssertConcurrent is mandatory for multi-input SP sends —
    // without it, finish_with_keys returns SilentPaymentRequiresInputBinding.
    spends_multi.finish_with_keys(ctx, &deltas_multi, Relation::AssertConcurrent, &multi_pks)?;
    sim.spend_coins(ctx.take(), &[sender_a.sk.clone(), sender_b.sk.clone()])?;
    println!(
        "Stage 6/9 — Sent 700 mojos via multi-input SP send (2 coins, Relation::AssertConcurrent)."
    );

    // Stage 7: extract via the canonical tweak_data_from_block_spends helper
    // over the simulator's block accessors. This is the same code path
    // real-block callers use after generator decompression — the SDK is
    // simulator-agnostic at this layer.
    let multi_spends = sim.block_spends(height_before_multi);
    let multi_additions = sim.block_outputs(height_before_multi);
    let tweak_data_multi = tweak_data_from_block_spends(&multi_spends, &multi_additions)?;
    println!(
        "Stage 7/9 — Extracted TweakData via tweak_data_from_block_spends: {} tweak_point(s), {} output(s).",
        tweak_data_multi.tweak_points.len(),
        tweak_data_multi.outputs.len(),
    );

    // Stage 8: scan and detect.
    let detections_multi = recipient.scan(&tweak_data_multi, None, K_MAX_DEFAULT);
    println!(
        "Stage 8/9 — Multi-input scan: detected {} output(s).",
        detections_multi.len()
    );

    // Stage 9: spend the detected multi-input coin.
    for d in &detections_multi {
        let synthetic_secret = d.onetime_sk.derive_synthetic();
        let conditions = Conditions::new()
            .create_coin(sender_a.puzzle_hash, d.amount - 1, Memos::None)
            .reserve_fee(1);
        let coin = Coin::new(d.parent_coin_id, d.puzzle_hash, d.amount);
        StandardLayer::new(synthetic_secret.public_key()).spend(ctx, coin, conditions)?;
        sim.spend_coins(ctx.take(), std::slice::from_ref(&synthetic_secret))?;
        println!("Stage 9/9 — Spent multi-input detected coin {}.", d.coin_id);
    }

    Ok(())
}
