//! The protocol primitives the scanner and the send path share (CHIP-0057 "Sending", "Scanning
//! a Spend Group", "Tweak Points").

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_puzzle_types::DeriveSynthetic;
use chia_puzzle_types::standard::StandardArgs;
use chia_sdk_types::silent_payments::{
    CHIA_SP_INPUTS, CHIA_SP_SHARED_SECRET, ScalarField, tagged_hash,
};
use chia_sdk_utils::silent_payments::SilentPaymentError;
use chia_sha2::Sha256;

use crate::DriverError;

/// The scanner's shared secret for a tweak point: `SHA256(serialize(scan_sk * tweak_point))`.
/// `tweak_point` must be a valid non-identity point; [`super::scan_from_tweaks`] checks this.
#[must_use]
pub fn compute_shared_secret_from_tweak(scan_sk: &SecretKey, tweak_point: &PublicKey) -> [u8; 32] {
    // The scalar bytes are held in a `ScalarField`, which zeroizes them on drop.
    let scalar = ScalarField::from_bytes_raw(scan_sk.to_bytes());
    let mut point = *tweak_point;
    point.scalar_multiply(scalar.as_bytes());
    let mut h = Sha256::new();
    h.update(point.to_bytes());
    h.finalize()
}

/// The output tweak `t_k` of output index `k`:
/// `int(tagged_hash("Chia_SP/SharedSecret", shared_secret || ser32(k))) mod r`.
#[must_use]
pub fn derive_output_tweak(shared_secret: &[u8; 32], k: u32) -> ScalarField {
    let mut data = [0u8; 36];
    data[..32].copy_from_slice(shared_secret);
    data[32..].copy_from_slice(&k.to_be_bytes());
    let hash = tagged_hash(CHIA_SP_SHARED_SECRET, &data);
    ScalarField::from_bytes_unsigned(hash)
}

/// The one-time public key of an output: `spend_pk + tweak * G`. Returns
/// [`DriverError::SilentPaymentScalarOutOfRange`] if `tweak` is not below the group order `r`.
pub fn derive_onetime_pk(
    spend_pk: &PublicKey,
    tweak: &ScalarField,
) -> Result<PublicKey, DriverError> {
    Ok(spend_pk + &tweak_secret_key(tweak)?.public_key())
}

/// The one-time secret key of an output: `(spend_sk + tweak) mod r`, where `tweak` is the
/// combined tweak of a detection (CHIP-0057 "Spending"). Fails like [`derive_onetime_pk`].
pub fn derive_onetime_sk(
    spend_sk: &SecretKey,
    tweak: &ScalarField,
) -> Result<SecretKey, DriverError> {
    Ok(spend_sk + &tweak_secret_key(tweak)?)
}

/// `tweak` as a `chia-bls` secret key, which accepts only values below `r`.
fn tweak_secret_key(tweak: &ScalarField) -> Result<SecretKey, DriverError> {
    SecretKey::from_bytes(tweak.as_bytes()).map_err(|_| DriverError::SilentPaymentScalarOutOfRange)
}

/// The standard puzzle hash of a one-time public key. The synthetic offset
/// uses the signed interpretation of the hash, as Chia's standard puzzle does.
#[must_use]
pub fn puzzle_hash_for_pk(pk: &PublicKey) -> Bytes32 {
    let synthetic = pk.derive_synthetic();
    StandardArgs::curry_tree_hash(synthetic).into()
}

// Send side. Callers pass synthetic secret keys; see `SyntheticSecretKey`.

/// The sum of the synthetic secret keys of a spend group, mod `r`, with one term per coin.
/// Returns [`DriverError::SilentPaymentZeroKeySum`] if the sum is zero or `sks` is empty.
pub fn aggregate_sender_sks(sks: &[SecretKey]) -> Result<SecretKey, DriverError> {
    let mut sum = SecretKey::from_bytes(&[0u8; 32]).expect("zero is a valid chia-bls scalar");
    for sk in sks {
        sum += sk;
    }
    if is_zero_key(&sum) {
        return Err(DriverError::SilentPaymentZeroKeySum);
    }
    Ok(sum)
}

/// Whether a secret key is zero, which `chia-bls` accepts as a secret key.
fn is_zero_key(sk: &SecretKey) -> bool {
    ScalarField::from_bytes_raw(sk.to_bytes()).is_zero()
}

/// The input hash of a spend group:
/// `int(tagged_hash("Chia_SP/Inputs", coin_id_L || serialize(A_sum))) mod r`, with `coin_id_L` the
/// smallest of `coin_ids`. Returns [`DriverError::SilentPaymentNoXchInputs`] if there are none.
pub fn compute_input_hash(
    coin_ids: &[Bytes32],
    aggregated_sender_pk: &PublicKey,
) -> Result<ScalarField, DriverError> {
    let coin_id_min = coin_ids
        .iter()
        .min()
        .ok_or(DriverError::SilentPaymentNoXchInputs)?;
    let pk_bytes = aggregated_sender_pk.to_bytes();

    let mut data = [0u8; 80];
    data[..32].copy_from_slice(coin_id_min.as_ref());
    data[32..].copy_from_slice(&pk_bytes);

    let hash = tagged_hash(CHIA_SP_INPUTS, &data);
    Ok(ScalarField::from_bytes_unsigned(hash))
}

/// The tweak point of a spend group, `T = input_hash * A_sum` (CHIP-0057 "Tweak Points").
/// `None` if `A_sum` is the identity element, `input_hash` is zero, or `coin_ids` is empty.
#[must_use]
pub fn compute_tweak_point(
    coin_ids: &[Bytes32],
    aggregated_sender_pk: &PublicKey,
) -> Option<PublicKey> {
    if aggregated_sender_pk.is_inf() {
        return None;
    }
    let input_hash = compute_input_hash(coin_ids, aggregated_sender_pk).ok()?;
    tweak_point_from_input_hash(aggregated_sender_pk, &input_hash)
}

/// `input_hash * A_sum`, or `None` if `A_sum` is the identity element or `input_hash` is zero.
pub(super) fn tweak_point_from_input_hash(
    a_sum: &PublicKey,
    input_hash: &ScalarField,
) -> Option<PublicKey> {
    if a_sum.is_inf() || input_hash.is_zero() {
        return None;
    }
    let mut tweak_point = *a_sum;
    tweak_point.scalar_multiply(input_hash.as_bytes());
    if tweak_point.is_inf() {
        return None;
    }
    Some(tweak_point)
}

/// The puzzle hash of the sender's `k`-th output to `scan_pk` in a spend group (CHIP-0057
/// `SendSilentPayment`), from [`aggregate_sender_sks`] and [`compute_input_hash`] of the group.
/// Fails if a public key is the identity element, or the key sum, input hash or output tweak
/// is zero. Subgroup membership is not checked: validate untrusted keys first, as
/// [`SilentPaymentAddress::validate`](chia_sdk_utils::silent_payments::SilentPaymentAddress::validate) does.
pub fn derive_one_time_puzzle_hash(
    scan_pk: &PublicKey,
    spend_pk: &PublicKey,
    aggregated_sender_sk: &SecretKey,
    input_hash: &ScalarField,
    k: u32,
) -> Result<Bytes32, DriverError> {
    // An identity key would make the shared secret public or let the sender spend the output.
    if scan_pk.is_inf() || spend_pk.is_inf() {
        return Err(DriverError::SilentPayment(
            SilentPaymentError::IdentityPublicKey,
        ));
    }
    let shared_secret = sender_shared_secret(scan_pk, aggregated_sender_sk, input_hash)?;
    let t_k = derive_output_tweak(&shared_secret, k);
    one_time_puzzle_hash_from_tweak(spend_pk, &t_k)
}

/// The sender's side of the ECDH, `SHA256(serialize(input_hash * (a_sum * B_scan)))`: two point
/// multiplications, so the key sum stays inside `chia-bls`. Fails if either scalar is zero.
fn sender_shared_secret(
    scan_pk: &PublicKey,
    aggregated_sender_sk: &SecretKey,
    input_hash: &ScalarField,
) -> Result<[u8; 32], DriverError> {
    if is_zero_key(aggregated_sender_sk) {
        return Err(DriverError::SilentPaymentZeroKeySum);
    }
    if input_hash.is_zero() {
        return Err(DriverError::SilentPaymentZeroInputHash);
    }

    let a_sum = ScalarField::from_bytes_raw(aggregated_sender_sk.to_bytes());
    let mut point = *scan_pk;
    point.scalar_multiply(a_sum.as_bytes());
    point.scalar_multiply(input_hash.as_bytes());
    let mut h = Sha256::new();
    h.update(point.to_bytes());
    Ok(h.finalize())
}

/// `puzzle_hash_for_pk(B_m + t_k * G)`, failing if `t_k` is zero (the one-time
/// key would then equal the address's own spend key).
fn one_time_puzzle_hash_from_tweak(
    spend_pk: &PublicKey,
    t_k: &ScalarField,
) -> Result<Bytes32, DriverError> {
    if t_k.is_zero() {
        return Err(DriverError::SilentPaymentZeroTweak);
    }
    let onetime_pk = derive_onetime_pk(spend_pk, t_k)?;
    Ok(puzzle_hash_for_pk(&onetime_pk))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chia_bls::SecretKey;
    use chia_sdk_types::silent_payments::GROUP_ORDER;
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

        let aggregated = aggregate_sender_sks(&[sk0, sk1]).expect("non-zero sum");

        assert_eq!(
            aggregated.to_bytes(),
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

        let result = compute_input_hash(&coin_ids, &pk).unwrap();

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

        let with_a_only = compute_input_hash(&[coin_a], &pk).unwrap();
        let with_both_b_first = compute_input_hash(&[coin_b, coin_a], &pk).unwrap();

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

        let ab = compute_input_hash(&[coin_a, coin_b], &pk).unwrap();
        let ba = compute_input_hash(&[coin_b, coin_a], &pk).unwrap();

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
        let aggregated_sender_sk = SecretKey::from_bytes(&TV1_AGGREGATED_SENDER_SK).unwrap();
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);

        let result =
            derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &aggregated_sender_sk, &input_hash, 0)
                .expect("TV1 derivation");

        assert_eq!(
            *result.as_ref(),
            TV1_PUZZLE_HASH,
            "TV1 round-trip: sender-side derive_one_time_puzzle_hash \
             must match the scanner's tv1_scan_detects_unlabeled_k0 result"
        );
    }

    /// At k=1 the sender's derivation agrees with the receiver's.
    ///
    /// Re-derives the expected puzzle hash via the receiver-side chain
    /// (`compute_shared_secret_from_tweak`, `derive_output_tweak(.., 1)`,
    /// `derive_onetime_pk`, `puzzle_hash_for_pk`) over the TV1 inputs, then
    /// asserts byte-equality against `derive_one_time_puzzle_hash(.., k=1)`.
    #[test]
    fn derive_one_time_puzzle_hash_k1_round_trip() {
        // b_*-style shorthand keeps clippy::similar_names quiet without an
        // `#[allow]` attribute (the scan vs spend pair differs by one byte
        // under longer names).
        let b_scan = SecretKey::from_bytes(&TV1_SCAN_SK).expect("TV1 scan_sk");
        let b_scan_pub = PublicKey::from_bytes(&TV1_SCAN_PK).expect("TV1 scan_pk");
        let b_spend_pub = PublicKey::from_bytes(&TV1_SPEND_PK).expect("TV1 spend_pk");
        let a_sum_sk = SecretKey::from_bytes(&TV1_AGGREGATED_SENDER_SK).unwrap();
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);

        // Receiver-side recomputation: construct the tweak_point the receiver
        // sees (input_hash * A_sum), then compute the shared_secret, then
        // derive the expected k=1 puzzle_hash via the protocol-primitive chain.
        let a_sum_pub = a_sum_sk.public_key();
        let mut tweak_point = a_sum_pub;
        tweak_point.scalar_multiply(input_hash.as_bytes());
        let expected_shared_secret = compute_shared_secret_from_tweak(&b_scan, &tweak_point);
        let expected_tweak = derive_output_tweak(&expected_shared_secret, 1);
        let expected_onetime_pk = derive_onetime_pk(&b_spend_pub, &expected_tweak).unwrap();
        let expected_ph = puzzle_hash_for_pk(&expected_onetime_pk);

        // Sender-side derivation under test:
        let sender_ph =
            derive_one_time_puzzle_hash(&b_scan_pub, &b_spend_pub, &a_sum_sk, &input_hash, 1)
                .expect("k=1 derivation");

        assert_eq!(
            *sender_ph.as_ref(),
            *expected_ph.as_ref(),
            "k=1 round-trip: sender and receiver derivations must agree byte-for-byte"
        );
    }

    // ─── CHIP-0057 "Edge Cases": zero key sum and zero scalars ───────────

    /// `r - sk`, the additive inverse of a secret key mod r.
    fn negate(sk: &SecretKey) -> SecretKey {
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
        SecretKey::from_bytes(&out).expect("r - sk is below r")
    }

    /// Secret keys `a` and `r - a` sum to zero mod r: the sender must fail.
    #[test]
    fn aggregate_sender_sks_rejects_zero_sum() {
        let a = SecretKey::from_seed(&[1u8; 32]);
        let minus_a = negate(&a);
        assert!((a.public_key() + &minus_a.public_key()).is_inf());

        let result = aggregate_sender_sks(&[a, minus_a]);
        assert!(matches!(result, Err(DriverError::SilentPaymentZeroKeySum)));
    }

    /// No keys at all also sum to zero.
    #[test]
    fn aggregate_sender_sks_rejects_empty() {
        let result = aggregate_sender_sks(&[]);
        assert!(matches!(result, Err(DriverError::SilentPaymentZeroKeySum)));
    }

    /// A zero key sum handed directly to the derivation is rejected as well.
    #[test]
    fn derive_one_time_puzzle_hash_rejects_zero_key_sum() {
        let scan_pk = PublicKey::from_bytes(&TV1_SCAN_PK).unwrap();
        let spend_pk = PublicKey::from_bytes(&TV1_SPEND_PK).unwrap();
        let zero = SecretKey::from_bytes(&[0u8; 32]).unwrap();
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);

        let result = derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &zero, &input_hash, 0);
        assert!(matches!(result, Err(DriverError::SilentPaymentZeroKeySum)));
    }

    /// An identity scan key or spend key makes the sender fail (CHIP-0057
    /// `SendSilentPayment`), whatever the other inputs.
    #[test]
    fn derive_one_time_puzzle_hash_rejects_identity_keys() {
        let scan_pk = PublicKey::from_bytes(&TV1_SCAN_PK).unwrap();
        let spend_pk = PublicKey::from_bytes(&TV1_SPEND_PK).unwrap();
        let a_sum = SecretKey::from_bytes(&TV1_AGGREGATED_SENDER_SK).unwrap();
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);
        let identity = PublicKey::default();
        assert!(identity.is_inf());

        for (scan, spend) in [(&identity, &spend_pk), (&scan_pk, &identity)] {
            let result = derive_one_time_puzzle_hash(scan, spend, &a_sum, &input_hash, 0);
            assert!(matches!(
                result,
                Err(DriverError::SilentPayment(
                    SilentPaymentError::IdentityPublicKey
                ))
            ));
        }
        assert_eq!(
            derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &a_sum, &input_hash, 0).unwrap(),
            Bytes32::from(TV1_PUZZLE_HASH)
        );
    }

    /// `input_hash == 0` makes the sender fail. A hash that reduces to zero
    /// cannot be produced on demand, so the zero scalar is passed in directly.
    #[test]
    fn derive_one_time_puzzle_hash_rejects_zero_input_hash() {
        let scan_pk = PublicKey::from_bytes(&TV1_SCAN_PK).unwrap();
        let spend_pk = PublicKey::from_bytes(&TV1_SPEND_PK).unwrap();
        let a_sum = SecretKey::from_bytes(&TV1_AGGREGATED_SENDER_SK).unwrap();
        let zero = ScalarField::from_bytes_raw([0u8; 32]);

        let result = derive_one_time_puzzle_hash(&scan_pk, &spend_pk, &a_sum, &zero, 0);
        assert!(matches!(
            result,
            Err(DriverError::SilentPaymentZeroInputHash)
        ));
    }

    /// `t_k == 0` makes the sender fail. As above, the zero tweak is passed to
    /// the helper that `derive_one_time_puzzle_hash` runs on every tweak.
    #[test]
    fn one_time_puzzle_hash_rejects_zero_tweak() {
        let spend_pk = PublicKey::from_bytes(&TV1_SPEND_PK).unwrap();
        let zero = ScalarField::from_bytes_raw([0u8; 32]);

        let result = one_time_puzzle_hash_from_tweak(&spend_pk, &zero);
        assert!(matches!(result, Err(DriverError::SilentPaymentZeroTweak)));

        // A non-zero tweak passes and matches the public composition.
        let t = derive_output_tweak(&TV1_SHARED_SECRET, 0);
        assert_eq!(
            one_time_puzzle_hash_from_tweak(&spend_pk, &t).unwrap(),
            Bytes32::from(TV1_PUZZLE_HASH)
        );
    }

    // ─── compute_tweak_point (CHIP-0057 "Tweak Points") ──────────────────

    /// `tweak_point` of `vector_1_single_output` in the CHIP's
    /// machine-readable vectors (one input).
    const TV1_TWEAK_POINT: [u8; 48] = hex!(
        "b9662882e0596df3c5af1d27b85d62677bb5389a6a1c1f4a1485034804b0ec60"
        "749e57a7f1e9712176dfab60298ff292"
    );
    /// `a_sum_pk`, the two coin ids and `tweak_point` of `vector_4_multi_input`.
    const TV4_A_SUM: [u8; 48] = hex!(
        "a223ab27f801044cd98c8314014b8073347b0e5aae43c69b78b5ca2a562ee9f7"
        "99b8efad179b34da1b306ca4d62bad40"
    );
    const TV4_COIN_ID_0: [u8; 32] =
        hex!("2b9857e0307ebfbe51829e3be8c992ae57f6a8debe06a5deab429ddae83a8c1a");
    const TV4_COIN_ID_1: [u8; 32] =
        hex!("209bb03a4cd165785e6149bc6dcb27e35829006f02ec927ab5a20521fd27d21a");
    const TV4_TWEAK_POINT: [u8; 48] = hex!(
        "82caa41f9b5e0675cea58072185286e956ec209f1df3a2c23b832c45b8bdd139"
        "bab6230e3d5ddc663d269440931bafeb"
    );

    #[test]
    fn compute_tweak_point_matches_the_vectors() {
        let a = PublicKey::from_bytes(&TV1_A_SUM).unwrap();
        let point = compute_tweak_point(&[Bytes32::new(TV1_COIN_ID)], &a).unwrap();
        assert_eq!(point.to_bytes(), TV1_TWEAK_POINT);
        // It is the point the scanner multiplies by its scan key.
        assert_eq!(point, tv1_tweak_point());
        assert_eq!(
            compute_shared_secret_from_tweak(&scan_sk(), &point),
            TV1_SHARED_SECRET
        );

        // Two inputs: the smaller coin id is used, whatever the order.
        let a_sum = PublicKey::from_bytes(&TV4_A_SUM).unwrap();
        let ids = [Bytes32::new(TV4_COIN_ID_0), Bytes32::new(TV4_COIN_ID_1)];
        let reversed = [ids[1], ids[0]];
        for coin_ids in [ids, reversed] {
            assert_eq!(
                compute_tweak_point(&coin_ids, &a_sum).unwrap().to_bytes(),
                TV4_TWEAK_POINT
            );
        }
    }

    /// Groups that are left out of a block's tweak points yield `None`: an
    /// identity key sum, and a zero input hash (which cannot be produced from
    /// a real hash, so the helper is given the zero scalar).
    #[test]
    fn compute_tweak_point_leaves_out_skipped_groups() {
        let coin_ids = [Bytes32::new(TV1_COIN_ID)];
        let a = PublicKey::from_bytes(&TV1_A_SUM).unwrap();

        assert!(compute_tweak_point(&coin_ids, &PublicKey::default()).is_none());
        assert!(compute_tweak_point(&[], &a).is_none());

        let zero = ScalarField::from_bytes_raw([0u8; 32]);
        assert!(tweak_point_from_input_hash(&a, &zero).is_none());
        let input_hash = ScalarField::from_bytes_unsigned(TV1_INPUT_HASH);
        assert!(tweak_point_from_input_hash(&PublicKey::default(), &input_hash).is_none());
        assert_eq!(
            tweak_point_from_input_hash(&a, &input_hash)
                .unwrap()
                .to_bytes(),
            TV1_TWEAK_POINT
        );
    }

    /// An empty coin list is an error, not a panic: a spend group has at
    /// least one coin.
    #[test]
    fn compute_input_hash_rejects_empty_coin_ids() {
        let pk = PublicKey::from_bytes(&TV1_A_SUM).unwrap();
        assert!(matches!(
            compute_input_hash(&[], &pk),
            Err(DriverError::SilentPaymentNoXchInputs)
        ));
    }

    /// `ScalarField::from_bytes_raw` does not reduce, so it can hold a value
    /// that is not below `r`. The one-time key derivations refuse it instead
    /// of panicking, and accept the largest valid scalar, `r - 1`.
    #[test]
    fn onetime_keys_reject_scalar_not_below_r() {
        let b_spend = SecretKey::from_bytes(&TV1_SCAN_SK).unwrap();
        let b_spend_pub = b_spend.public_key();

        for bytes in [GROUP_ORDER, [0xff; 32]] {
            let tweak = ScalarField::from_bytes_raw(bytes);
            assert!(matches!(
                derive_onetime_pk(&b_spend_pub, &tweak),
                Err(DriverError::SilentPaymentScalarOutOfRange)
            ));
            assert!(matches!(
                derive_onetime_sk(&b_spend, &tweak),
                Err(DriverError::SilentPaymentScalarOutOfRange)
            ));
        }

        let mut r_minus_1 = GROUP_ORDER;
        r_minus_1[31] -= 1;
        let tweak = ScalarField::from_bytes_raw(r_minus_1);
        let onetime_sk = derive_onetime_sk(&b_spend, &tweak).unwrap();
        assert_eq!(
            onetime_sk.public_key(),
            derive_onetime_pk(&b_spend_pub, &tweak).unwrap()
        );
    }
}
