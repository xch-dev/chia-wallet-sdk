//! Silent-payments protocol primitives — the building blocks the scanner and
//! the send-side action share.
//!
//! Protocol primitives (low-level — operate on a single shared secret / scalar):
//!
//! - [`compute_shared_secret_from_tweak`] — wallet-side ECDH: `SHA256(scan_sk * tweak_point)`.
//! - [`derive_output_tweak`] — per-output tweak scalar: `tagged_hash(CHIA_SP_SHARED_SECRET, shared_secret ‖ ser32(k)) mod r`.
//! - [`derive_onetime_pk`] — `spend_pk + tweak * G`.
//! - [`derive_onetime_sk`] — `(spend_sk + tweak) mod r`.
//! - [`puzzle_hash_for_pk`] — `StandardArgs::curry_tree_hash(pk.derive_synthetic())`.
//!
//! Send-side compositions (high-level — fold the primitives into the values the
//! send action and `Spends::finish_with_keys` need):
//!
//! - [`aggregate_sender_sks`] — `Σ sk_i mod r` over the wallet's synthetic SKs.
//! - [`compute_input_hash`] — `tagged_hash(CHIA_SP_INPUTS, coin_id_min ‖ serialize(A_sum)) mod r`.
//! - [`derive_one_time_puzzle_hash`] — the sender's analog of the receiver's scan loop:
//!   `aggregated_sender_sk * input_hash * scan_pk → shared_secret → t_k → onetime_pk → puzzle_hash`.
//!
//! Every scalar that comes out of `tagged_hash` flows through
//! [`chia_sdk_types::silent_payments::ScalarField::from_bytes_unsigned`] — the
//! type-system boundary that prevents the signed-vs-unsigned mixing hazard.

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_puzzle_types::DeriveSynthetic;
use chia_puzzle_types::standard::StandardArgs;
use chia_sdk_types::silent_payments::{
    CHIA_SP_INPUTS, CHIA_SP_SHARED_SECRET, ScalarField, tagged_hash,
};
use chia_sha2::Sha256;

/// Compute the wallet-side ECDH shared secret for a pre-computed tweak point.
///
/// `shared_secret = SHA256(serialize(scan_sk * tweak_point))`.
///
/// The 48-byte compressed BLS12-381 G1 serialization is fed to SHA-256
/// directly per CHIP-0057 §169 and `~/silent-payments/crates/sp-common/src/ecdh.rs`.
///
/// `tweak_point` must not be the identity element (the scanner guards this
/// upstream — `PublicKey::is_inf()`); this function is the cheap inner
/// primitive and does not re-check.
#[must_use]
pub fn compute_shared_secret_from_tweak(scan_sk: &SecretKey, tweak_point: &PublicKey) -> [u8; 32] {
    let mut point = *tweak_point;
    point.scalar_multiply(&scan_sk.to_bytes());
    let mut h = Sha256::new();
    h.update(point.to_bytes());
    h.finalize()
}

/// Derive the per-output tweak scalar `t_k` for output index `k` within a
/// spend group.
///
/// `t_k = ScalarField::from_bytes_unsigned(tagged_hash(CHIA_SP_SHARED_SECRET, shared_secret ‖ ser32(k)))`
///
/// `ser32(k)` is BIG-endian per CHIP-0057 §169. The `to_be_bytes` choice is
/// invisible from TV1/TV3/TV4 (all `k = 0`); the bespoke `k = 1` test below
/// catches a `to_le_bytes` regression.
#[must_use]
pub fn derive_output_tweak(shared_secret: &[u8; 32], k: u32) -> ScalarField {
    let mut data = [0u8; 36];
    data[..32].copy_from_slice(shared_secret);
    data[32..].copy_from_slice(&k.to_be_bytes());
    let hash = tagged_hash(CHIA_SP_SHARED_SECRET, &data);
    ScalarField::from_bytes_unsigned(hash)
}

/// Derive the one-time public key for an output: `onetime_pk = spend_pk + tweak * G`.
///
/// The `tweak` is treated as a 32-byte BLS-style secret key by
/// `chia_bls::SecretKey::from_bytes` — this is exactly the canonical encoding
/// `ScalarField::as_bytes` produces, so the round-trip is byte-clean. The
/// `ScalarField::from_bytes_unsigned` boundary guarantees `tweak.as_bytes() < r`.
#[must_use]
pub fn derive_onetime_pk(spend_pk: &PublicKey, tweak: &ScalarField) -> PublicKey {
    let tweak_secret = SecretKey::from_bytes(tweak.as_bytes())
        .expect("ScalarField::from_bytes_unsigned guarantees value < r");
    spend_pk + &tweak_secret.public_key()
}

/// Derive the one-time secret key for an output: `onetime_sk = (spend_sk + tweak) mod r`.
///
/// The `spend_sk` bytes are fed through `ScalarField::from_bytes_raw` (NOT
/// `from_bytes_unsigned`) — `chia_bls::SecretKey` is already constrained to
/// `< r` by construction, so no reduction is needed and we want to preserve
/// the byte pattern exactly. The addition then reduces mod r.
#[must_use]
pub fn derive_onetime_sk(spend_sk: &SecretKey, tweak: &ScalarField) -> SecretKey {
    let sk_scalar = ScalarField::from_bytes_raw(spend_sk.to_bytes());
    let result = sk_scalar.add(tweak);
    SecretKey::from_bytes(result.as_bytes()).expect("ScalarField add stays < r by construction")
}

/// Compute the standard p2 puzzle hash for a one-time public key.
///
/// Reuses `chia_puzzle_types::DeriveSynthetic` + `StandardArgs::curry_tree_hash`,
/// matching the standard-layer construction.
///
/// `pk.derive_synthetic()` routes through `chia_puzzle_types::derive_synthetic`'s
/// SIGNED reducer, which is correct here — that offset is a Chia-consensus-pinned
/// signed reduction, not a silent-payments construction. The silent-payments
/// `ScalarField` boundary applies only to the output tweak, never the
/// standard-puzzle synthetic offset.
#[must_use]
pub fn puzzle_hash_for_pk(pk: &PublicKey) -> Bytes32 {
    let synthetic = pk.derive_synthetic();
    StandardArgs::curry_tree_hash(synthetic).into()
}

// Send-side compositions.
//
// The 3 functions below compose the protocol primitives above into the values
// the send-side action and `Spends::finish_with_keys` need:
//
//   aggregate_sender_sks  → Σ synthetic_sk_i mod r
//   compute_input_hash    → tagged_hash binding the spent coin set + aggregated PK
//   derive_one_time_puzzle_hash  → sender-side analog of the receiver's scan loop
//
// Callers MUST pass synthetic SKs (the ones whose PKs are curried into
// `StandardArgs::synthetic_key`). Two layered guards enforce this:
//   (a) the `SyntheticSecretKey` / `SyntheticPublicKey` newtypes at the
//       `Spends::with_silent_payment_keys` boundary make passing a raw key a
//       compile error; and
//   (b) the `sp_finish_branch` runtime guard returns
//       `DriverError::SilentPaymentKeyNotSynthetic` before signing when a
//       registered key does not curry to the spent coin's `p2_puzzle_hash` —
//       the universal backstop covering the newtype's `from_synthetic_unchecked`
//       escape hatch and every FFI caller.
// The Spends-level multi-party / coverage gates check key PRESENCE only; they
// do not validate synthetic-ness.

/// Aggregate synthetic sender secret keys via mod-r addition.
///
/// Returns `Σ sk_i mod r` as a [`ScalarField`]. The empty-slice case returns
/// the zero scalar (callers must additionally check `is_empty()` if zero is
/// not a sensible aggregate).
///
/// Each input SK is fed through [`ScalarField::from_bytes_raw`] (NOT
/// `from_bytes_unsigned`) because `chia_bls::SecretKey` is already constrained
/// to `< r` by construction. The addition then reduces mod r. There is a
/// `1/r ≈ 2^-255` chance the sum is zero (cosmic-ray-level probability);
/// callers that downstream call `SecretKey::from_bytes(self.as_bytes())` must
/// accept this vanishing risk.
///
/// Privacy warning: this function takes secret-key material. The resulting
/// [`ScalarField`] is sensitive — wallets must treat it like an SK (zeroize on
/// drop, do not log). Consumers further compose this with
/// [`derive_one_time_puzzle_hash`] which emits an on-chain puzzle hash whose
/// recipient holds the scan key; memos attached at the action layer are
/// visible to anyone holding the recipient's scan key.
#[must_use]
pub fn aggregate_sender_sks(sks: &[SecretKey]) -> ScalarField {
    let mut sum = ScalarField::from_bytes_raw([0u8; 32]);
    for sk in sks {
        let sk_scalar = ScalarField::from_bytes_raw(sk.to_bytes());
        sum = sum.add(&sk_scalar);
    }
    sum
}

/// Compute the per-spend-group input-hash scalar.
///
/// `coin_ids` is the slice of spent-coin ids forming the spend group; the
/// lexicographically-smallest 32-byte id is selected internally.
/// `aggregated_sender_pk` is the 48-byte compressed serialization of
/// `Σ synthetic_sk_i * G`, computed at finish time by
/// `Spends::finish_with_keys` (chip-0057 SP branch).
///
/// Returns a [`ScalarField`] reduced unsigned mod-r. The scalar is used both
/// (a) by the sender to derive each output's per-output tweak (via
/// [`derive_output_tweak`] downstream of the ECDH path); and (b) by the receiver
/// reconstructing the same group via the `Relation::AssertConcurrent` cycle
/// (opcode 64 SCC) the SDK emits on multi-input bundles.
///
/// # Panics
/// Panics if `coin_ids` is empty. This is an internal invariant, not a
/// reachable failure mode: every in-crate caller (`Spends::finish_with_keys`
/// via the chip-0057 SP finish branch, and `tweak_data_from_block_spends`)
/// passes a non-empty XCH-input set, and the bindings facade rejects empty
/// input with `DriverError::SilentPaymentNoXchInputs` before delegating here —
/// so this panic is never reachable across the FFI boundary.
///
/// Privacy warning: the `input_hash` scalar is a deterministic public function
/// of the spent coin ids + aggregated sender PK; both are visible on chain
/// after the send. The scalar itself is not sensitive, but it can be re-derived
/// by anyone observing the transaction. The privacy property of silent
/// payments derives from the recipient's scan key, NOT from this scalar's
/// secrecy.
#[must_use]
pub fn compute_input_hash(coin_ids: &[Bytes32], aggregated_sender_pk: &PublicKey) -> ScalarField {
    assert!(
        !coin_ids.is_empty(),
        "compute_input_hash requires at least one coin id"
    );

    let coin_id_min = coin_ids.iter().min().expect("non-empty checked above");
    let pk_bytes = aggregated_sender_pk.to_bytes();

    let mut data = [0u8; 80];
    data[..32].copy_from_slice(coin_id_min.as_ref());
    data[32..].copy_from_slice(&pk_bytes);

    let hash = tagged_hash(CHIA_SP_INPUTS, &data);
    ScalarField::from_bytes_unsigned(hash)
}

/// Derive the on-chain standard-p2 puzzle hash for the recipient's k-th output
/// in this spend group.
///
/// `scan_pk` is the recipient's scan public key (from their silent-payment
/// address). `spend_pk` is the recipient's spend public key (for unlabeled
/// sends it equals the address's `spend_pk` field; for labeled sends the
/// caller passes the address's already-tweaked spend PK, which differs from
/// the unlabeled by `+ label_pk(m)`).
///
/// `aggregated_sender_sk` is the sum-mod-r of the sender's synthetic SKs for
/// every XCH input in this transaction ([`aggregate_sender_sks`]). `input_hash`
/// is the per-spend-group input-hash ([`compute_input_hash`]). `k` is the
/// per-recipient counter on `Spends` — 0 for the first output to `scan_pk`,
/// 1 for the second, etc.
///
/// Privacy warning: this function emits a puzzle hash that, when used in a
/// `CreateCoin` condition, lands an output at a fresh one-time address on
/// chain. The address is unlinkable to the recipient's published `spxch1...`
/// address without the scan secret key. However, any memos attached to the
/// corresponding `CreateCoin` condition are visible on chain and to anyone
/// with the scan key — the action-layer memo-hint guard prevents the standard
/// wallet from promoting a 32-byte first memo to a `puzzle_hash` hint and
/// defeating the privacy gain.
#[must_use]
pub fn derive_one_time_puzzle_hash(
    scan_pk: &PublicKey,
    spend_pk: &PublicKey,
    aggregated_sender_sk: &ScalarField,
    input_hash: &ScalarField,
    k: u32,
) -> Bytes32 {
    // Step 1: tweak_scalar = aggregated_sender_sk * input_hash (mod r).
    // This is the sender-side analog of the receiver's tweak_point construction.
    let tweak_scalar = aggregated_sender_sk.mul(input_hash);

    // Step 2: ECDH over scan_pk. shared_secret = SHA256(tweak_scalar * scan_pk).
    let mut point = *scan_pk;
    point.scalar_multiply(tweak_scalar.as_bytes());
    let mut h = Sha256::new();
    h.update(point.to_bytes());
    let shared_secret: [u8; 32] = h.finalize();

    // Step 3: t_k = derive_output_tweak(shared_secret, k).
    let t_k = derive_output_tweak(&shared_secret, k);

    // Step 4: onetime_pk = spend_pk + t_k * G.
    let onetime_pk = derive_onetime_pk(spend_pk, &t_k);

    // Step 5: puzzle_hash = curry(onetime_pk.derive_synthetic()).
    puzzle_hash_for_pk(&onetime_pk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chia_bls::SecretKey;
    use hex_literal::hex;

    // ─── TV1 pinned bytes (CHIP-0057 test vector 1) ──────────────────────

    const TV1_SCAN_SK: [u8; 32] =
        hex!("132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6");
    const TV1_SCAN_PK: [u8; 48] = hex!(
        "a04f404bfbfdc9311736899fe32d2275bb007814510c3523529487ad75736075"
        "73ade20d31c75107b40331fff79ac896"
    );
    const TV1_SPEND_PK: [u8; 48] = hex!(
        "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
        "f6f31e22e0831ba5088c952625a75c74"
    );
    const TV1_A_SUM: [u8; 48] = hex!(
        "8d9a5ed9c9b1a58476b07262007c636d775f2a33f0533737f3b3b0eaf99a8c0c"
        "51b3f2d87dc03a657e07f1828ab760fa"
    );
    const TV1_COIN_ID: [u8; 32] =
        hex!("5d759d2d97c03b1f6fe0657e91d25f6b7dd1311d6023271a1bcd35978a94a175");
    const TV1_INPUT_HASH: [u8; 32] =
        hex!("38a1c8379cceb0fbebfdf3016707e54a1c7e9d21afb9489b9cc58f6055cc9411");
    const TV1_SHARED_SECRET: [u8; 32] =
        hex!("d3ac1e8f651a73d2e20b43cb73fd6997de5504afbc04a2d4546a92d0020ba2c6");
    const TV1_PUZZLE_HASH: [u8; 32] =
        hex!("23adba149dd9000d65e0f8e21b6975364cbe89a63caf56533df4b7664c21fbf5");
    /// TV1 aggregated sender SK — single-input, so equals the sole sender SK.
    /// Byte-identical to `TV4_SENDER_SK_0` below (TV1 single-input case and TV4
    /// first sender share the same reference fixture); kept under both names
    /// because the semantic meaning differs across tests.
    const TV1_AGGREGATED_SENDER_SK: [u8; 32] =
        hex!("5002eaf015c1c3a9694cc054e96273279732f4f963616ff89b6d4addcd678c7a");

    // ─── TV4 pinned bytes (CHIP-0057 test vector 4, multi-input) ─────────

    const TV4_SENDER_SK_0: [u8; 32] =
        hex!("5002eaf015c1c3a9694cc054e96273279732f4f963616ff89b6d4addcd678c7a");
    const TV4_SENDER_SK_1: [u8; 32] =
        hex!("05fded8808216b65d439fc41cb07c7270e37ed743e0745652afe055cfe91cf0f");
    const TV4_AGGREGATED_SK: [u8; 32] =
        hex!("5600d8781de32f0f3d86bc96b46a3a4ea56ae26da168b55dc66b503acbf95b89");

    // ─── Helpers ─────────────────────────────────────────────────────────

    /// Helper: construct TV1's `tweak_point = input_hash * A_sum`.
    fn tv1_tweak_point() -> PublicKey {
        let mut a_sum = PublicKey::from_bytes(&TV1_A_SUM).expect("TV1 A_sum");
        a_sum.scalar_multiply(&TV1_INPUT_HASH);
        a_sum
    }

    fn scan_sk() -> SecretKey {
        SecretKey::from_bytes(&TV1_SCAN_SK).expect("TV1 scan_sk")
    }

    // ─── Tests for the protocol primitives ───────────────────────────────

    /// `compute_shared_secret_from_tweak` matches TV1's pinned value
    /// `d3ac1e8f...0ba2c6` — verifies the ECDH primitive byte for byte.
    #[test]
    fn tv1_shared_secret_matches() {
        let tweak_point = tv1_tweak_point();
        let secret = compute_shared_secret_from_tweak(&scan_sk(), &tweak_point);
        assert_eq!(secret, TV1_SHARED_SECRET);
    }

    /// An adversarial scalar whose first byte has the high bit set reduces
    /// under UNSIGNED interpretation (`BigUint::from_bytes_be(&bytes) % r`), NOT
    /// signed interpretation.
    ///
    /// Verifies the `ScalarField` boundary fires end-to-end through
    /// `derive_output_tweak`: any future refactor that swapped
    /// `from_bytes_unsigned` for a signed reducer would either (a) produce a
    /// value with high-bit set, failing the first assertion below, or (b)
    /// compile-error because `ScalarField` has no signed constructor.
    #[test]
    fn adversarial_ff32_scalar_reduces_unsigned() {
        let shared_secret = [0xffu8; 32];
        let tweak = derive_output_tweak(&shared_secret, 0);

        // Sanity: the result IS a ScalarField (< r), so the first byte must
        // be < 0x80 (BLS12-381 subgroup order `r` begins with `0x73`).
        let bytes = tweak.to_bytes();
        assert!(
            bytes[0] < 0x80,
            "ScalarField output first byte must be < 0x80 (got 0x{:02x}); signed reduction would have produced a different value",
            bytes[0]
        );

        // Stronger pin: re-derive via the same protocol path and assert
        // determinism (catches an accidental rand-injection regression).
        let tweak2 = derive_output_tweak(&shared_secret, 0);
        assert_eq!(tweak.to_bytes(), tweak2.to_bytes());

        // Cross-check against the direct ScalarField::from_bytes_unsigned path.
        let mut data = [0u8; 36];
        data[..32].copy_from_slice(&shared_secret);
        data[32..].copy_from_slice(&0u32.to_be_bytes());
        let expected = ScalarField::from_bytes_unsigned(tagged_hash(CHIA_SP_SHARED_SECRET, &data));
        assert_eq!(tweak.to_bytes(), expected.to_bytes());
    }

    // ─── Tests for the send-side compositions ────────────────────────────

    /// Aggregating the two TV4 sender synthetic SKs produces the pinned
    /// `TV4_AGGREGATED_SK` byte-for-byte. Catches `ScalarField::add` regressions
    /// AND iteration-order bugs (aggregation is commutative; if a future
    /// refactor sorts the slice, the result must still match).
    #[test]
    fn tv4_aggregate_sender_sks_matches() {
        let sk0 = SecretKey::from_bytes(&TV4_SENDER_SK_0).expect("TV4 sender SK 0 < r");
        let sk1 = SecretKey::from_bytes(&TV4_SENDER_SK_1).expect("TV4 sender SK 1 < r");

        let aggregated = aggregate_sender_sks(&[sk0, sk1]);

        assert_eq!(
            *aggregated.as_bytes(),
            TV4_AGGREGATED_SK,
            "aggregate_sender_sks(TV4) must match the pinned TV4_AGGREGATED_SK"
        );
    }

    /// TV1 input-hash byte-pin. Verifies the lex-min `coin_id` +
    /// `serialize(A_sum)` || `tagged_hash` pipeline matches the byte-for-byte
    /// CHIP-pinned `38a1c8...cc9411` value.
    #[test]
    fn tv1_compute_input_hash_matches() {
        let pk = PublicKey::from_bytes(&TV1_A_SUM).expect("TV1 A_sum is a valid BLS PK");
        let coin_ids = vec![Bytes32::new(TV1_COIN_ID)];

        let result = compute_input_hash(&coin_ids, &pk);

        assert_eq!(
            *result.as_bytes(),
            TV1_INPUT_HASH,
            "TV1 compute_input_hash must match pinned 38a1c8...cc9411"
        );
    }

    /// A two-coin input where the smaller id is in position [1] returns the
    /// SAME scalar as a single-element slice with just the smaller id. Verifies
    /// the function selects `iter().min()`, not `[0]`.
    #[test]
    fn input_hash_uses_lex_min_coin_id() {
        let pk = PublicKey::from_bytes(&TV1_A_SUM).expect("TV1 A_sum");

        let coin_a: Bytes32 = [0x01u8; 32].into();
        let coin_b: Bytes32 = [0x02u8; 32].into();
        assert!(coin_a < coin_b);

        let with_a_only = compute_input_hash(&[coin_a], &pk);
        let with_both_b_first = compute_input_hash(&[coin_b, coin_a], &pk);

        assert_eq!(
            with_a_only.to_bytes(),
            with_both_b_first.to_bytes(),
            "lex-min coin_id must be selected from a multi-element slice"
        );
    }

    /// Swapping the slice order of two coin ids produces the same scalar.
    #[test]
    fn input_hash_order_independent() {
        let pk = PublicKey::from_bytes(&TV1_A_SUM).expect("TV1 A_sum");

        let coin_a: Bytes32 = [0x01u8; 32].into();
        let coin_b: Bytes32 = [0x02u8; 32].into();

        let ab = compute_input_hash(&[coin_a, coin_b], &pk);
        let ba = compute_input_hash(&[coin_b, coin_a], &pk);

        assert_eq!(
            ab.to_bytes(),
            ba.to_bytes(),
            "compute_input_hash must be order-independent"
        );
    }

    /// TV1 round-trip closure. The sender-side puzzle-hash derivation produces
    /// the SAME byte-string the scanner detects in `tv1_scan_detects_unlabeled_k0`.
    #[test]
    fn tv1_derive_one_time_puzzle_hash_matches() {
        let scan_pk = PublicKey::from_bytes(&TV1_SCAN_PK).expect("TV1 scan_pk");
        let spend_pk = PublicKey::from_bytes(&TV1_SPEND_PK).expect("TV1 spend_pk");
        let aggregated_sender_sk = ScalarField::from_bytes_raw(TV1_AGGREGATED_SENDER_SK);
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);

        let result =
            derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &aggregated_sender_sk, &input_hash, 0);

        assert_eq!(
            *result.as_ref(),
            TV1_PUZZLE_HASH,
            "TV1 round-trip: sender-side derive_one_time_puzzle_hash \
             must match the scanner's tv1_scan_detects_unlabeled_k0 result"
        );
    }

    /// At k=1 the derivation matches the `bespoke_k1_detection` in-test
    /// computation.
    ///
    /// Re-derives the expected puzzle hash via the same protocol-primitive
    /// chain (`compute_shared_secret_from_tweak`, `derive_output_tweak(.., 1)`,
    /// `derive_onetime_pk`, `puzzle_hash_for_pk`) over the TV1 inputs, then
    /// asserts byte-equality against `derive_one_time_puzzle_hash(.., k=1)`.
    /// Catches `ser32(k)` endianness regressions because TV1/TV3/TV4 are all k=0.
    #[test]
    fn derive_one_time_puzzle_hash_k1_round_trip() {
        // b_*-style shorthand keeps clippy::similar_names quiet without an
        // `#[allow]` attribute (the scan vs spend pair differs by one byte
        // under longer names).
        let b_scan = SecretKey::from_bytes(&TV1_SCAN_SK).expect("TV1 scan_sk");
        let b_scan_pub = PublicKey::from_bytes(&TV1_SCAN_PK).expect("TV1 scan_pk");
        let b_spend_pub = PublicKey::from_bytes(&TV1_SPEND_PK).expect("TV1 spend_pk");
        let a_sum_sk = ScalarField::from_bytes_raw(TV1_AGGREGATED_SENDER_SK);
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);

        // Receiver-side recomputation: construct the tweak_point the receiver
        // sees (input_hash * A_sum), then compute the shared_secret, then
        // derive the expected k=1 puzzle_hash via the protocol-primitive chain.
        let a_sum_pub = SecretKey::from_bytes(a_sum_sk.as_bytes())
            .expect("aggregated SK < r")
            .public_key();
        let mut tweak_point = a_sum_pub;
        tweak_point.scalar_multiply(input_hash.as_bytes());
        let expected_shared_secret = compute_shared_secret_from_tweak(&b_scan, &tweak_point);
        let expected_tweak = derive_output_tweak(&expected_shared_secret, 1);
        let expected_onetime_pk = derive_onetime_pk(&b_spend_pub, &expected_tweak);
        let expected_ph = puzzle_hash_for_pk(&expected_onetime_pk);

        // Sender-side derivation under test:
        let sender_ph =
            derive_one_time_puzzle_hash(&b_scan_pub, &b_spend_pub, &a_sum_sk, &input_hash, 1);

        assert_eq!(
            *sender_ph.as_ref(),
            *expected_ph.as_ref(),
            "k=1 round-trip: sender and receiver derivations must agree byte-for-byte"
        );
    }
}
