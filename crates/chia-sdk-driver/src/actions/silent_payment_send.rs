use chia_puzzle_types::Memos;
use chia_sdk_utils::silent_payments::SilentPaymentAddress;

use crate::{
    Asset, BURN_PUZZLE_HASH, Deltas, DriverError, Id, Output, SpendAction, SpendContext, Spends,
    silent_payments::SilentPaymentPending,
};

/// CHIP-0057 silent-payment send action (chip-0057-gated). Structurally
/// XCH-only: there is no `Id` field, so there is no non-XCH footgun.
///
/// Privacy warning: `memos` is published on-chain in plaintext and visible
/// to anyone holding the recipient's scan key. A 32-byte first memo is
/// rejected at apply time (`DriverError::SilentPaymentMemoHintForbidden`).
#[derive(Debug, Clone)]
pub struct SilentPaymentSendAction {
    pub recipient: SilentPaymentAddress,
    pub amount: u64,
    pub memos: Memos,
}

impl SilentPaymentSendAction {
    pub fn new(recipient: SilentPaymentAddress, amount: u64, memos: Memos) -> Self {
        Self {
            recipient,
            amount,
            memos,
        }
    }
}

impl SpendAction for SilentPaymentSendAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        deltas.update(Id::Xch).output += self.amount;
        deltas.set_needed(Id::Xch);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        memo_hint_guard(ctx, self.memos)?;
        spend_silent_payment(ctx, spends, &self.recipient, self.amount, self.memos)
    }
}

/// Apply-time half of a chip-0057 silent-payment send: reserves an XCH parent,
/// increments the per-`scan_pk` k counter on `Spends`, and pushes a
/// `SilentPaymentPending` entry. NO `CreateCoin` is emitted here — the
/// on-chain output is emitted at finish time by the chip-0057 SP branch of
/// [`Spends::finish_with_keys`], which derives the one-time puzzle hash from
/// the recorded entry.
///
/// Privacy warning: memos passed here land in the on-chain `CreateCoin.memos`
/// field unchanged at finish time and are visible to anyone holding the
/// recipient's scan key. The 32-byte first-memo hint guard
/// (`DriverError::SilentPaymentMemoHintForbidden`) is fired by the caller
/// before this helper runs.
fn spend_silent_payment(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    recipient: &SilentPaymentAddress,
    amount: u64,
    memos: Memos,
) -> Result<(), DriverError> {
    // 1. Reserve XCH parent. BURN_PUZZLE_HASH is the placeholder puzzle hash
    //    because output_source only uses `amount` for source-selection
    //    arithmetic; the real puzzle hash arrives at finish time. Avoiding
    //    Bytes32::default() prevents a plausible-looking all-zeros collision.
    let output = Output::new(BURN_PUZZLE_HASH, amount);
    let source = spends.xch.output_source(ctx, &output)?;
    let parent = &spends.xch.items[source];
    let parent_coin_id = parent.asset.coin_id();
    let parent_puzzle_hash = parent.asset.full_puzzle_hash();

    // 2. Per-scan_pk k counter. Keyed by 48-byte compressed scan_pk so
    //    distinct sub-addresses (labeled vs unlabeled) of the same recipient
    //    share a counter.
    let scan_pk_bytes: [u8; 48] = recipient.scan_pk.to_bytes();
    let next_k = spends
        .silent_payment_counters
        .entry(scan_pk_bytes)
        .or_insert(0);
    let k = *next_k;
    *next_k += 1;

    // 3. Push pending entry. ECDH math + CreateCoin emission + outputs.xch
    //    push are all deferred to the chip-0057 SP branch of
    //    Spends::finish_with_keys.
    spends.silent_payments_pending.push(SilentPaymentPending {
        scan_pk: recipient.scan_pk,
        spend_pk: recipient.spend_pk,
        parent_xch_index: source,
        parent_coin_id,
        parent_puzzle_hash,
        k,
        amount,
        memos,
    });

    Ok(())
}

/// Reject a 32-byte first memo that would be promoted to a `puzzle_hash` hint
/// by the standard Chia wallet, defeating silent-payment privacy.
///
/// Privacy warning: the standard wallet (Sage, the mainnet wallet) treats a
/// 32-byte first memo as a `puzzle_hash` hint and indexes the output by that
/// hash — exposing the one-time puzzle hash to any indexer. For silent
/// payments, this completely defeats the privacy gain.
///
/// Returns `Err(DriverError::SilentPaymentMemoHintForbidden)` if the first
/// memo atom is exactly 32 bytes. All other memo shapes (`Memos::None`, non-pair,
/// non-atom head, first atom != 32 bytes, malformed) return `Ok(())`.
fn memo_hint_guard(ctx: &SpendContext, memos: Memos) -> Result<(), DriverError> {
    use clvmr::SExp;

    let Memos::Some(ptr) = memos else {
        return Ok(());
    };

    let SExp::Pair(head, _tail) = ctx.sexp(ptr) else {
        return Ok(());
    };

    let SExp::Atom = ctx.sexp(head) else {
        return Ok(());
    };

    let atom = ctx.atom(head);
    if atom.as_ref().len() == 32 {
        return Err(DriverError::SilentPaymentMemoHintForbidden);
    }

    Ok(())
}

#[cfg(all(test, feature = "chip-0057"))]
mod silent_payment_tests {
    use anyhow::Result;
    use chia_bls::SecretKey;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_utils::silent_payments::{SilentPaymentAddress, SilentPaymentNetwork};
    use indexmap::indexmap;

    use crate::{
        Action, DriverError, Relation, SpendContext, Spends,
        silent_payments::{
            SyntheticPublicKey, SyntheticSecretKey, aggregate_sender_sks, compute_input_hash,
            derive_one_time_puzzle_hash,
        },
    };

    // ====================================================================
    // Acceptance tests for the silent-payment send path. Each drives
    // `Action::silent_payment_send(..)` through `with_silent_payment_keys` +
    // `finish_with_keys`.
    // ====================================================================

    /// Apply-time state: after `spends.apply(&[Action::silent_payment_send(...)])`,
    /// the action records one entry on `spends.silent_payments_pending` with the
    /// matching fields. ECDH and `CreateCoin` emission are NOT inspected here.
    #[test]
    fn action_state_machine() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        // Build a recipient address from arbitrary BLS keys (the action does
        // not perform ECDH at apply time, so the keys' relationship to any
        // real wallet is irrelevant for this state-machine test).
        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let expected_scan_pk = recipient_scan_sk.public_key();
        let expected_spend_pk = recipient_spend_sk.public_key();
        let recipient = SilentPaymentAddress::new(
            expected_scan_pk,
            expected_spend_pk,
            SilentPaymentNetwork::Mainnet,
        );

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let _deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, Memos::None)],
        )?;

        assert_eq!(
            spends.silent_payments_pending.len(),
            1,
            "exactly one pending entry after one Action::silent_payment_send(...)"
        );

        let pending = &spends.silent_payments_pending[0];
        assert_eq!(pending.scan_pk, expected_scan_pk);
        assert_eq!(pending.spend_pk, expected_spend_pk);
        assert_eq!(pending.amount, 1);
        assert_eq!(pending.k, 0, "k-counter starts at 0");
        assert!(
            pending.parent_xch_index < spends.xch.items.len(),
            "parent_xch_index is a valid index into spends.xch.items"
        );

        // Counter recorded for this scan_pk:
        let scan_pk_bytes: [u8; 48] = expected_scan_pk.to_bytes();
        assert_eq!(
            spends.silent_payment_counters.get(&scan_pk_bytes).copied(),
            Some(1),
            "counter was incremented past the recorded k value"
        );

        Ok(())
    }

    /// Finish-time round-trip: the apply+finish flow produces an XCH output
    /// whose `puzzle_hash` matches what `derive_one_time_puzzle_hash`
    /// independently computes for the same `(scan_pk, spend_pk,
    /// aggregated_sender_sk, input_hash, k=0)` tuple. Exercises the
    /// `Action::silent_payment_send` + `Spends::finish_with_keys` construction
    /// shape.
    #[test]
    fn round_trip_matches_derive_one_time_puzzle_hash() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        // Recipient address from arbitrary BLS keys. In the BlsPair fixture the
        // "synthetic" SK == raw SK; the math closes because both sides of the
        // round-trip use the same interpretation.
        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // Capture the recipient's keys before the address moves into the action.
        let scan_pk = recipient.scan_pk;
        let spend_pk = recipient.spend_pk;

        // Apply + finish via the new unified API.
        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, Memos::None)],
        )?;

        // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap
        // the raw fixture key via `from_synthetic_unchecked` (coin curried over
        // the raw pk).
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let synthetic_public_map = indexmap! { alice.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(alice.pk) };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(alice.sk.clone()),
        };
        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let outputs = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map)?;

        // Independently compute the expected one-time puzzle hash via the
        // free functions (Vec intermediate to satisfy clippy on slice
        // construction; functionally identical to passing one SK through
        // aggregate_sender_sks).
        let alice_sks = vec![alice.sk.clone()];
        let aggregated_sender_sk = aggregate_sender_sks(&alice_sks);
        let agg_pk = SecretKey::from_bytes(aggregated_sender_sk.as_bytes())
            .expect("aggregated SK < r")
            .public_key();
        let input_hash = compute_input_hash(&[alice.coin.coin_id()], &agg_pk);
        let expected_ph =
            derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &aggregated_sender_sk, &input_hash, 0);

        // Assert: at least one xch output matches expected_ph + amount 1.
        // outputs.xch may also include change (alice.coin amount > 1).
        let found = outputs
            .xch
            .iter()
            .any(|c| c.puzzle_hash == expected_ph && c.amount == 1);
        assert!(
            found,
            "expected an output at puzzle_hash {} amount 1; got outputs.xch = {:?}",
            hex::encode(expected_ph),
            outputs.xch
        );

        Ok(())
    }

    /// Two `Action::silent_payment_send` calls with the same SP recipient in one
    /// `Spends` produce outputs at k=0 and k=1 respectively. The counter on
    /// `spends.silent_payment_counters` increments per `scan_pk`.
    #[test]
    fn multi_output_same_scan_pk_increments_k() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(10);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        // Capture scan_pk before recipient is moved into the actions.
        let scan_pk = recipient.scan_pk;

        let _deltas = spends.apply(
            &mut ctx,
            &[
                Action::silent_payment_send(recipient.clone(), 1, Memos::None),
                Action::silent_payment_send(recipient, 2, Memos::None),
            ],
        )?;

        assert_eq!(spends.silent_payments_pending.len(), 2);
        assert_eq!(
            spends.silent_payments_pending[0].k, 0,
            "first output to recipient is k=0"
        );
        assert_eq!(
            spends.silent_payments_pending[1].k, 1,
            "second output to same scan_pk is k=1"
        );
        assert_eq!(spends.silent_payments_pending[0].amount, 1);
        assert_eq!(spends.silent_payments_pending[1].amount, 2);

        let scan_pk_bytes: [u8; 48] = scan_pk.to_bytes();
        assert_eq!(
            spends.silent_payment_counters.get(&scan_pk_bytes).copied(),
            Some(2),
            "counter incremented past k=1"
        );

        Ok(())
    }

    /// Two `Action::silent_payment_send` calls with DIFFERENT SP recipients in
    /// one `Spends` produce outputs both at k=0 (per-`scan_pk` counters are
    /// independent).
    #[test]
    fn multi_output_distinct_scan_pks_independent_counters() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(10);

        let recipient_a = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );
        let recipient_b = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x44u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x45u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let _deltas = spends.apply(
            &mut ctx,
            &[
                Action::silent_payment_send(recipient_a, 1, Memos::None),
                Action::silent_payment_send(recipient_b, 2, Memos::None),
            ],
        )?;

        assert_eq!(spends.silent_payments_pending.len(), 2);
        assert_eq!(
            spends.silent_payments_pending[0].k, 0,
            "recipient_a's first output is k=0"
        );
        assert_eq!(
            spends.silent_payments_pending[1].k, 0,
            "recipient_b's first output is k=0 (independent counter)"
        );

        Ok(())
    }

    /// The receiver's `compute_input_hash` over the on-chain `coin_id`s plus the
    /// aggregated synthetic PK reconstructs the SAME `input_hash` the sender
    /// used. Uses the `Relation::AssertConcurrent` cycle binding for the 2-input
    /// case.
    #[test]
    fn input_hash_round_trip() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);
        let bob = sim.bls(7);

        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        let scan_pk = recipient.scan_pk;
        let spend_pk = recipient.spend_pk;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);
        spends.add(bob.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, Memos::None)],
        )?;

        // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap
        // the raw fixture keys via `from_synthetic_unchecked` (coins curried
        // over the raw pk).
        let pk_map = indexmap! {
            alice.puzzle_hash => alice.pk,
            bob.puzzle_hash => bob.pk,
        };
        let synthetic_public_map = indexmap! {
            alice.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(alice.pk),
            bob.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(bob.pk),
        };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(alice.sk.clone()),
            bob.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(bob.sk.clone()),
        };
        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let outputs =
            spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map)?;

        // Independent reconstruction of the expected puzzle hash via the
        // free functions — exactly the path the scanner would follow after the
        // cycle binding (opcode-64 SCC) provides the input set.
        //
        // Vec intermediate (clippy::cloned_ref_to_slice_refs precedent):
        // satisfies clippy on the slice construction.
        let sender_sks = vec![alice.sk.clone(), bob.sk.clone()];
        let aggregated_sender_sk = aggregate_sender_sks(&sender_sks);
        let agg_pk = SecretKey::from_bytes(aggregated_sender_sk.as_bytes())
            .expect("aggregated SK < r")
            .public_key();
        let coin_ids = vec![alice.coin.coin_id(), bob.coin.coin_id()];
        let input_hash = compute_input_hash(&coin_ids, &agg_pk);
        let expected_ph =
            derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &aggregated_sender_sk, &input_hash, 0);

        let found = outputs
            .xch
            .iter()
            .any(|c| c.puzzle_hash == expected_ph && c.amount == 1);
        assert!(
            found,
            "input_hash round-trip failed: expected puzzle_hash {} amount 1 not in outputs",
            hex::encode(expected_ph)
        );

        Ok(())
    }

    /// Passing a 32-byte first memo to `Action::silent_payment_send`
    /// errors at apply time with `DriverError::SilentPaymentMemoHintForbidden`.
    /// The action's side-effects on `Spends` (parent reservation, k-counter
    /// increment, `SilentPaymentPending` push) DO NOT happen because the guard
    /// fires before them.
    #[test]
    fn memo_hint_guard_rejects_32_byte_first_memo() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // Construct Memos with a 32-byte first atom via the canonical
        // `ctx.hint(...)` helper — it builds a Memos<NodePtr> containing
        // exactly one 32-byte atom (the Bytes32 hint).
        let hint_bytes: chia_protocol::Bytes32 = [0xffu8; 32].into();
        let bad_memos = ctx.hint(hint_bytes)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, bad_memos)],
        );

        assert!(
            matches!(result, Err(DriverError::SilentPaymentMemoHintForbidden)),
            "expected SilentPaymentMemoHintForbidden, got {result:?}"
        );

        // No side effects: pending list still empty (apply failed before
        // the SilentPaymentPending push).
        assert!(
            spends.silent_payments_pending.is_empty(),
            "guard must fire BEFORE pushing SilentPaymentPending"
        );

        Ok(())
    }

    /// A 1-byte sentinel followed by a 32-byte payload passes the guard — the
    /// first memo is 1 byte, not 32. This is the wallet
    /// author's explicit escape hatch if they legitimately need a 32-byte
    /// payload memo: prefix it with a sentinel byte so the first atom is
    /// no longer 32 bytes.
    #[test]
    fn memo_hint_guard_allows_sentinel_prefixed() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // Build Memos with a 1-byte first atom + a 32-byte second atom.
        let sentinel: chia_protocol::Bytes = chia_protocol::Bytes::new(vec![0x00u8]);
        let payload: chia_protocol::Bytes = chia_protocol::Bytes::new(vec![0xffu8; 32]);
        let safe_memos = ctx.memos(&[sentinel, payload])?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, safe_memos)],
        );

        assert!(
            result.is_ok(),
            "1-byte sentinel + 32-byte payload must pass: {result:?}"
        );
        assert_eq!(spends.silent_payments_pending.len(), 1);

        Ok(())
    }

    /// `Memos::None` passes the guard trivially.
    #[test]
    fn memo_hint_guard_allows_none() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let result = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, Memos::None)],
        );

        assert!(result.is_ok(), "Memos::None must pass: {result:?}");
        assert_eq!(spends.silent_payments_pending.len(), 1);

        Ok(())
    }

    // ====================================================================
    // Finish-time key-registration acceptance test.
    // ====================================================================

    /// An SP send applied without a prior `with_silent_payment_keys` call
    /// returns `Err(DriverError::SilentPaymentKeysNotRegistered)` at finish time.
    #[test]
    fn silent_payment_keys_not_registered_errors_at_finish() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1);

        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(recipient, 1, Memos::None)],
        )?;

        // DELIBERATELY DO NOT CALL with_silent_payment_keys.

        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let result = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None, // single-input — binding gate short-circuits; KeysNotRegistered fires
            &pk_map,
        );

        assert!(
            matches!(result, Err(DriverError::SilentPaymentKeysNotRegistered)),
            "expected SilentPaymentKeysNotRegistered when finish runs without with_silent_payment_keys, got: {result:?}"
        );
        Ok(())
    }
}
