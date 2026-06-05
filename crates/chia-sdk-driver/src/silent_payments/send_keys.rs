//! The `SyntheticSecretKey` / `SyntheticPublicKey` newtypes that the
//! silent-payment send path uses to prove a key is the synthetic key (the one
//! curried into `StandardArgs::synthetic_key`).
//!
//! The finish-time derivation pipeline (aggregate SKs, compute `input_hash`,
//! derive one-time puzzle hash, emit `CreateCoin`) lives in the chip-0057 SP
//! branch of [`crate::Spends::finish_with_keys`] via the private
//! `sp_finish_branch` helper in `action_system/spends.rs`, which also enforces
//! the multi-input atomic binding via [`crate::Relation::AssertConcurrent`].

use chia_bls::{PublicKey, SecretKey};
use chia_puzzle_types::DeriveSynthetic;

/// A `chia_bls::SecretKey` proven (by construction via [`SyntheticSecretKey::from_raw`]
/// or by the `sp_finish_branch` runtime check) to be the SYNTHETIC secret key —
/// the one whose public key is curried into `StandardArgs::synthetic_key`. The
/// CHIP-0057 send path aggregates these verbatim; passing a raw wallet SK lands
/// funds at an undetectable one-time puzzle hash, so the type exists to make that
/// a compile error.
#[derive(Debug, Clone)]
pub struct SyntheticSecretKey(SecretKey);

impl SyntheticSecretKey {
    /// Synthesize from a raw wallet SK via the DEFAULT hidden puzzle — the same
    /// `chia_puzzle_types::DeriveSynthetic` path `puzzle_hash_for_pk` uses, so
    /// outputs stay byte-identical to the standard-spend convention.
    #[must_use]
    pub fn from_raw(raw: &SecretKey) -> Self {
        Self(raw.derive_synthetic())
    }

    /// Escape hatch: wrap a key the caller asserts is already synthetic. NOT
    /// validated here — the runtime check in `sp_finish_branch`
    /// (`curry_tree_hash(pk) == coin p2_puzzle_hash` + `sk.public_key() == pk`)
    /// is the universal backstop that rejects a mis-wrapped key before signing.
    #[must_use]
    pub fn from_synthetic_unchecked(synthetic: SecretKey) -> Self {
        Self(synthetic)
    }

    /// The [`SyntheticPublicKey`] corresponding to this synthetic secret key.
    #[must_use]
    pub fn public_key(&self) -> SyntheticPublicKey {
        SyntheticPublicKey(self.0.public_key())
    }

    /// Consume the newtype and return the wrapped synthetic [`SecretKey`].
    #[must_use]
    pub fn into_inner(self) -> SecretKey {
        self.0
    }

    /// Borrow the wrapped synthetic [`SecretKey`].
    #[must_use]
    pub fn as_inner(&self) -> &SecretKey {
        &self.0
    }
}

/// A `chia_bls::PublicKey` proven (by construction via [`SyntheticPublicKey::from_raw`]
/// or by the `sp_finish_branch` runtime check) to be the SYNTHETIC public key —
/// the one curried into `StandardArgs::synthetic_key`. The CHIP-0057 send path
/// uses these verbatim; passing a raw wallet PK lands funds at an undetectable
/// one-time puzzle hash, so the type exists to make that a compile error.
#[derive(Debug, Clone, Copy)]
pub struct SyntheticPublicKey(PublicKey);

impl SyntheticPublicKey {
    /// Synthesize from a raw wallet PK via the DEFAULT hidden puzzle — the same
    /// `chia_puzzle_types::DeriveSynthetic` path `puzzle_hash_for_pk` uses, so
    /// outputs stay byte-identical to the standard-spend convention.
    #[must_use]
    pub fn from_raw(raw: &PublicKey) -> Self {
        Self(raw.derive_synthetic())
    }

    /// Escape hatch: wrap a key the caller asserts is already synthetic. NOT
    /// validated here — the runtime check in `sp_finish_branch`
    /// (`curry_tree_hash(pk) == coin p2_puzzle_hash`) is the universal backstop
    /// that rejects a mis-wrapped key before signing.
    #[must_use]
    pub fn from_synthetic_unchecked(synthetic: PublicKey) -> Self {
        Self(synthetic)
    }

    /// Consume the newtype and return the wrapped synthetic [`PublicKey`].
    #[must_use]
    pub fn into_inner(self) -> PublicKey {
        self.0
    }

    /// Borrow the wrapped synthetic [`PublicKey`].
    #[must_use]
    pub fn as_inner(&self) -> &PublicKey {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_bls::SecretKey;
    use chia_puzzle_types::{DeriveSynthetic, Memos};
    use chia_sdk_test::Simulator;
    use chia_sdk_utils::silent_payments::{SilentPaymentAddress, SilentPaymentNetwork};
    use indexmap::indexmap;

    use super::{SyntheticPublicKey, SyntheticSecretKey};
    use crate::{Action, DriverError, Relation, SpendContext, Spends};

    /// Byte-equality contract: the newtype `from_raw` constructors MUST route
    /// through the exact `chia_puzzle_types::DeriveSynthetic` path
    /// `puzzle_hash_for_pk` uses, and `from_synthetic_unchecked` MUST be
    /// bit-preserving — otherwise the one-time puzzle hash drifts from the
    /// detection oracle.
    #[test]
    fn newtype_from_raw_matches_derive_synthetic() -> Result<()> {
        let raw_secret = SecretKey::from_bytes(&[0x11u8; 32])?;
        let raw_public = raw_secret.public_key();

        // 1. from_raw(&raw_sk).into_inner() == derive_synthetic(&raw_sk)
        assert_eq!(
            SyntheticSecretKey::from_raw(&raw_secret)
                .into_inner()
                .to_bytes(),
            raw_secret.derive_synthetic().to_bytes(),
            "SyntheticSecretKey::from_raw must mirror DeriveSynthetic::derive_synthetic",
        );

        // 2. from_raw(&raw_sk).public_key().into_inner() == derive_synthetic(&raw_sk).public_key()
        assert_eq!(
            SyntheticSecretKey::from_raw(&raw_secret)
                .public_key()
                .into_inner()
                .to_bytes(),
            raw_secret.derive_synthetic().public_key().to_bytes(),
            "SyntheticSecretKey::public_key must equal the synthetic SK's public key",
        );

        // 3. from_synthetic_unchecked(k).into_inner() == k (no synthesis applied)
        assert_eq!(
            SyntheticSecretKey::from_synthetic_unchecked(raw_secret.clone())
                .into_inner()
                .to_bytes(),
            raw_secret.to_bytes(),
            "from_synthetic_unchecked must be bit-preserving (no derive_synthetic)",
        );

        // 4. SyntheticPublicKey::from_raw(&raw_pk).into_inner() == derive_synthetic(&raw_pk)
        assert_eq!(
            SyntheticPublicKey::from_raw(&raw_public)
                .into_inner()
                .to_bytes(),
            raw_public.derive_synthetic().to_bytes(),
            "SyntheticPublicKey::from_raw must mirror DeriveSynthetic::derive_synthetic",
        );

        Ok(())
    }

    /// Spends-level multi-party hard-error: a Spends with 2 non-ephemeral XCH
    /// inputs but only 1 in the registered SK map returns
    /// `Err(DriverError::SilentPaymentMultiPartyUnsupported)` — NOT a silent
    /// single-input aggregation (which would silently corrupt the puzzle hash).
    /// Multi-party flows are not currently supported.
    ///
    /// Uses `Action::silent_payment_send`, registers keys via
    /// `with_silent_payment_keys`, and finishes via `finish_with_keys`.
    #[test]
    fn multi_party_hard_errors() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = sim.bls(3);

        // Recipient address from arbitrary BLS keys.
        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // `pk_map` stays raw for `finish_with_keys` (used to spend the coins).
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        // The `sim.bls()` fixture coins are curried over the RAW pk, so the
        // registered SP key IS the raw key — wrap via `from_synthetic_unchecked`.
        let synthetic_public_map = indexmap! { alice.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(alice.pk) };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(alice.sk.clone()),
        };

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin); // wallet-controlled
        spends.add(bob.coin); // counterparty (NOT in sk_map)

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        // synthetic_secret_map contains ONLY Alice — Bob is missing.
        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result =
            spends.finish_with_keys(&mut ctx, &deltas, Relation::AssertConcurrent, &pk_map);

        assert!(
            matches!(result, Err(DriverError::SilentPaymentMultiPartyUnsupported)),
            "expected SilentPaymentMultiPartyUnsupported, got {result:?}"
        );

        Ok(())
    }

    /// A `Spends` with 2 wallet-controlled XCH inputs + 1 SP send MUST
    /// be passed `Relation::AssertConcurrent` to `finish_with_keys`. Anything
    /// else (including `Relation::None`) returns
    /// `Err(DriverError::SilentPaymentRequiresInputBinding)`.
    ///
    /// The SK-coverage check is NOT triggered: both Alice's and Bob's SKs are
    /// registered, so under `Relation::AssertConcurrent` the call would
    /// succeed. The input-binding gate is sequenced ahead of the SK-coverage
    /// check inside `sp_finish_branch` so a misconfigured multi-input send
    /// fails fast rather than producing detectable-but-unbound output coins.
    #[test]
    fn multi_input_requires_assert_concurrent_relation() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);
        let bob = sim.bls(7);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

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

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);
        spends.add(bob.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);

        assert!(
            matches!(result, Err(DriverError::SilentPaymentRequiresInputBinding)),
            "expected SilentPaymentRequiresInputBinding, got {result:?}"
        );

        Ok(())
    }

    /// A `Spends` with 1 XCH input + 1 SP send accepts
    /// `Relation::None` — the gate short-circuits because non-ephemeral XCH
    /// count < 2. Single-input SP sends do not require input binding.
    ///
    /// The success path exercises `sp_finish_branch` end-to-end (gates pass;
    /// derivation pipeline runs).
    #[test]
    fn single_input_accepts_relation_none() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // `pk_map` stays raw for `finish_with_keys`; the SP newtype maps wrap
        // the raw fixture key via `from_synthetic_unchecked`.
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let synthetic_public_map = indexmap! { alice.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(alice.pk) };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(alice.sk.clone()),
        };

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);

        assert!(
            result.is_ok(),
            "single-input SP send with Relation::None should succeed: {result:?}"
        );

        Ok(())
    }

    /// The runtime backstop against the raw-key footgun: a wallet that registers
    /// a key whose `StandardArgs::curry_tree_hash(pk)` does NOT equal the spent
    /// coin's `p2_puzzle_hash` MUST fail typed BEFORE signing.
    ///
    /// The `sim.bls()` fixture coin is curried over the RAW `alice.pk`, so
    /// registering `alice.pk.derive_synthetic()` makes
    /// `curry_tree_hash(derive_synthetic(alice.pk)) != alice.puzzle_hash` — the
    /// exact mismatch a raw-key mistake produces against a real standard-spend
    /// coin. Single input, `Relation::None`: the input-binding gate
    /// short-circuits, so the next gate to fire is the synthetic-key check.
    #[test]
    fn raw_pk_single_input_fails_not_synthetic() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // `pk_map` for `finish_with_keys` must contain a key that lets the
        // standard-spend lookup proceed; it is irrelevant to the SP branch,
        // which runs first inside `prepare`. Mismatched SP keys: the registered
        // pk is `derive_synthetic(alice.pk)`, but the coin is curried over the
        // raw `alice.pk`, so `curry_tree_hash(registered_pk) != ph`.
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let synthetic_public_map = indexmap! {
            alice.puzzle_hash =>
                SyntheticPublicKey::from_synthetic_unchecked(alice.pk.derive_synthetic()),
        };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash =>
                SyntheticSecretKey::from_synthetic_unchecked(alice.sk.derive_synthetic()),
        };

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);

        assert!(
            matches!(result, Err(DriverError::SilentPaymentKeyNotSynthetic)),
            "expected SilentPaymentKeyNotSynthetic for a raw-vs-coin mismatch, got {result:?}"
        );

        Ok(())
    }

    /// Synthetic-key check, second arm — sk/pk map consistency: a registered pk
    /// that DOES match the coin (`curry_tree_hash(pk) == ph`) but whose paired sk
    /// has a different public key (`sk.public_key() != pk`) MUST fail typed
    /// before signing. Catches a wallet that registers mismatched halves of the
    /// pk/sk maps.
    #[test]
    fn sk_pk_mismatch_fails_not_synthetic() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // pk matches the coin (raw `alice.pk`, curried as `alice.puzzle_hash`),
        // but the registered sk is an unrelated key, so `sk.public_key() != pk`.
        // The byte pattern is small (well under the BLS group order) so
        // `SecretKey::from_bytes` succeeds.
        let other_sk = SecretKey::from_bytes(&[0x01u8; 32])?;
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let synthetic_public_map = indexmap! {
            alice.puzzle_hash => SyntheticPublicKey::from_synthetic_unchecked(alice.pk),
        };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash => SyntheticSecretKey::from_synthetic_unchecked(other_sk),
        };

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);

        assert!(
            matches!(result, Err(DriverError::SilentPaymentKeyNotSynthetic)),
            "expected SilentPaymentKeyNotSynthetic for sk.public_key() != pk, got {result:?}"
        );

        Ok(())
    }

    /// The runtime check backstops the `SyntheticSecretKey::from_synthetic_unchecked`
    /// escape hatch: a key the caller WRONGLY asserts is synthetic (here
    /// `derive_synthetic(alice.pk)` against a coin curried over the raw
    /// `alice.pk`) is rejected typed before signing. Distinct contract from
    /// `raw_pk_single_input_fails_not_synthetic` — that pins the raw-key
    /// footgun, this pins the newtype escape-hatch backstop.
    #[test]
    fn unchecked_wrong_key_is_backstopped() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(5);

        let recipient_scan_sk = SecretKey::from_bytes(&[0x42u8; 32])?;
        let recipient_spend_sk = SecretKey::from_bytes(&[0x43u8; 32])?;
        let recipient = SilentPaymentAddress::new(
            recipient_scan_sk.public_key(),
            recipient_spend_sk.public_key(),
            SilentPaymentNetwork::Mainnet,
        );

        // The caller used `from_synthetic_unchecked` to assert a key is
        // synthetic when it is not (it double-synthesizes a key the coin
        // curried raw). The sp_finish_branch check catches the mis-assertion.
        let pk_map = indexmap! { alice.puzzle_hash => alice.pk };
        let synthetic_public_map = indexmap! {
            alice.puzzle_hash =>
                SyntheticPublicKey::from_synthetic_unchecked(alice.pk.derive_synthetic()),
        };
        let synthetic_secret_map = indexmap! {
            alice.puzzle_hash =>
                SyntheticSecretKey::from_synthetic_unchecked(alice.sk.derive_synthetic()),
        };

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::silent_payment_send(
                recipient.clone(),
                1,
                Memos::None,
            )],
        )?;

        spends.with_silent_payment_keys(synthetic_public_map, synthetic_secret_map);

        let result = spends.finish_with_keys(&mut ctx, &deltas, Relation::None, &pk_map);

        assert!(
            matches!(result, Err(DriverError::SilentPaymentKeyNotSynthetic)),
            "from_synthetic_unchecked of a wrong key must be backstopped by sp_finish_branch, got {result:?}"
        );

        Ok(())
    }
}
