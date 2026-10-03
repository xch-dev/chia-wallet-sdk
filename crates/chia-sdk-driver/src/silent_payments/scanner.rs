//! Transport-agnostic silent-payment scanner.
//!
//! Implements CHIP-0057's wallet-side detection: given a [`TweakData`] (pre-computed
//! tweak points + candidate output metadata), iterate `k = 0, 1, 2, ...` per spend
//! group, derive the candidate one-time puzzle hash, and emit a [`DetectedSpCoin`]
//! whenever it matches one of the candidate outputs.
//!
//! Two CHIP-mandated guards the reference impl is missing:
//!
//! - **CHIP §459 identity-element skip:** if `tweak_point.is_inf()`, skip silently.
//!   Without this, an adversarial indexer can produce a predictable shared secret
//!   and force false positives.
//! - **CHIP §416 `K_max` cap:** bounded `for k in 0..k_max` (not `loop { ... }`)
//!   prevents DOS by forged matches. Default `K_MAX_DEFAULT = 2400` per CHIP §446
//!   (the Chia mempool maximum number of silent-payment outputs per spend bundle).
//!
//! The labeled-detection branch (the CHIP-0057 labeled k-termination rule) is
//! interleaved with the unlabeled branch below: at each `k` the scanner first
//! checks the unlabeled candidate, then iterates the registered labels; the `k`
//! loop terminates only when BOTH the unlabeled candidate AND every labeled
//! candidate miss.

use std::collections::HashMap;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_sdk_types::silent_payments::{K_MAX, ScalarField};
use chia_sdk_utils::silent_payments::{
    CHANGE_LABEL, LabelRegistry, SilentPaymentKeys, generate_label,
};

use super::protocol::{
    compute_shared_secret_from_tweak, derive_onetime_pk, derive_output_tweak, puzzle_hash_for_pk,
};
use super::types::{DetectedSpCoin, OutputMeta, TweakData};

/// The per-spend-group iteration cap of the scanner: `K_max` of CHIP-0057
/// "Kmax: Maximum Outputs Per Spend Group" (2,400), as a `usize`.
///
/// Pass this to [`scan_from_tweaks`]. A smaller value makes the scan miss
/// outputs at higher indices that a conforming sender may have created; a
/// larger value is capped, since a scanner stops when `k` reaches `K_max`.
pub const K_MAX_DEFAULT: usize = K_MAX as usize;

/// Scan tweak points and candidate outputs for silent payments addressed to
/// this wallet (CHIP-0057 `ScanForSilentPayment`, driven by tweak points).
///
/// Scanning needs only the scan secret key and the spend public key, never the
/// spend secret key, so it can run on a watch-only device. Each detection
/// carries the combined tweak `(t_k + label_scalar) mod r`; the holder of the
/// spend secret key turns it into the one-time key with
/// [`DetectedSpCoin::onetime_sk`] (or [`super::derive_onetime_sk`]).
///
/// For each tweak point `T`, one ECDH is performed (`scan_sk * T`, hashed to
/// the shared secret) and `k = 0, 1, 2, ...` is iterated up to `k_max`. At each
/// `k` the unlabeled candidate is checked first, then the change label `m = 0`
/// (always, whether or not it is in `labels`; a match is reported as
/// `Some(0)`), then each label in `labels` in ascending order. The group is
/// abandoned at the first `k` where nothing matches. Every coin
/// with a matching puzzle hash is reported, since several output coins can
/// share one one-time puzzle hash (CHIP-0057 "Outputs Sharing a Puzzle Hash").
///
/// `k_max` is the iteration cap per spend group; pass [`K_MAX_DEFAULT`]. Values
/// above `K_max` are capped to it.
///
/// Tweak points are taken from another party, so each one is checked to be a
/// non-identity element of the prime-order subgroup before it is multiplied by
/// the scan key (CHIP-0057 "Tweak Points"); points that fail are skipped.
/// Outputs are matched against all of `data.outputs` (CHIP-0057 "Output
/// Matching Scope").
#[must_use]
pub fn scan_from_tweaks(
    scan_sk: &SecretKey,
    spend_pk: &PublicKey,
    data: &TweakData,
    labels: Option<&LabelRegistry>,
    k_max: usize,
) -> Vec<DetectedSpCoin> {
    let outputs = index_outputs(&data.outputs);
    let labels = label_set(scan_sk, labels);
    let mut detected = Vec::new();

    // A scanner stops iterating a spend group when k reaches K_max.
    let k_bound = u32::try_from(k_max).unwrap_or(u32::MAX).min(K_MAX);

    for tweak_point in &data.tweak_points {
        // CHIP-0057 "Tweak Points" / "Edge Cases": never multiply the scan key
        // by the identity element or by a point outside the prime-order subgroup.
        if tweak_point.is_inf() || !tweak_point.is_valid() {
            continue;
        }

        let shared_secret = compute_shared_secret_from_tweak(scan_sk, tweak_point);

        scan_group(
            |k| derive_output_tweak(&shared_secret, k),
            scan_sk,
            spend_pk,
            &outputs,
            &labels,
            k_bound,
            &mut detected,
        );
    }

    detected
}

/// The labels to scan for: the change label `m = 0`, which is always included
/// (CHIP-0057 "Scanning a Spend Group": the label set should always contain
/// it), followed by the caller's labels in ascending order of `m`.
fn label_set(scan_sk: &SecretKey, labels: Option<&LabelRegistry>) -> Vec<(u32, PublicKey)> {
    let (_, change_label_pk) = generate_label(scan_sk, CHANGE_LABEL);
    let mut set = vec![(CHANGE_LABEL, change_label_pk)];
    if let Some(registry) = labels {
        set.extend(
            registry
                .iter()
                .filter(|(m, _)| *m != CHANGE_LABEL)
                .map(|(m, label_pk)| (m, *label_pk)),
        );
    }
    set
}

/// The candidate outputs by puzzle hash. Several coins can share one puzzle
/// hash (they differ in parent or amount), and all of them are spendable with
/// the same one-time key.
fn index_outputs(outputs: &[OutputMeta]) -> HashMap<Bytes32, Vec<&OutputMeta>> {
    let mut index: HashMap<Bytes32, Vec<&OutputMeta>> = HashMap::new();
    for output in outputs {
        index.entry(output.puzzle_hash).or_default().push(output);
    }
    index
}

/// The `k` loop of the CHIP-0057 `ScanForSilentPayment` procedure for one spend
/// group. `tweak_for_k` yields the output tweak `t_k`; it is a parameter so that
/// the zero-tweak rule can be tested.
fn scan_group(
    tweak_for_k: impl Fn(u32) -> ScalarField,
    scan_sk: &SecretKey,
    spend_pk: &PublicKey,
    outputs: &HashMap<Bytes32, Vec<&OutputMeta>>,
    labels: &[(u32, PublicKey)],
    k_bound: u32,
    detected: &mut Vec<DetectedSpCoin>,
) {
    // Record a detection for every coin with the matched puzzle hash.
    let mut record = |coins: &[&OutputMeta], k: u32, label: Option<u32>, tweak: &ScalarField| {
        for coin in coins {
            detected.push(DetectedSpCoin {
                coin_id: coin.coin_id,
                puzzle_hash: coin.puzzle_hash,
                amount: coin.amount,
                parent_coin_id: coin.parent_coin_id,
                k,
                label,
                tweak: tweak.clone(),
            });
        }
    };

    for k in 0..k_bound {
        let output_tweak = tweak_for_k(k);

        // CHIP-0057 "Edge Cases": a zero tweak stops the scan of this group.
        if output_tweak.is_zero() {
            break;
        }

        let candidate_pk = derive_onetime_pk(spend_pk, &output_tweak);
        let candidate_hash = puzzle_hash_for_pk(&candidate_pk);

        if let Some(coins) = outputs.get(&candidate_hash) {
            record(coins, k, None, &output_tweak);
            continue;
        }

        // Labeled candidates are only checked when the unlabeled candidate at
        // this k missed. The first label that matches wins for this k; the
        // change label is checked first, then the others in ascending order.
        let mut found = false;
        for &(m, label_pk) in labels {
            let labeled_pk = candidate_pk + &label_pk;
            let labeled_hash = puzzle_hash_for_pk(&labeled_pk);
            if let Some(coins) = outputs.get(&labeled_hash) {
                // The label scalar is derived from the scan key, so the
                // combined tweak needs no spend key either.
                let (label_scalar, _) = generate_label(scan_sk, m);
                record(coins, k, Some(m), &output_tweak.add(&label_scalar));
                found = true;
                break;
            }
        }

        // Stop at the first index with no match.
        if !found {
            break;
        }
    }
}

/// Lets a [`SilentPaymentKeys`] bundle drive the scanner with a single method
/// call.
///
/// The type is defined in `chia-sdk-utils`, so the bundled flow is exposed as a
/// trait here. A watch-only scanner, which holds the scan secret key and the
/// spend public key but not the spend secret key, cannot build a
/// [`SilentPaymentKeys`] and calls [`scan_from_tweaks`] directly.
pub trait SilentPaymentScan {
    /// Scan `tweak_data` for silent-payment outputs addressed to this key
    /// bundle. Equivalent to calling [`scan_from_tweaks`] with this bundle's
    /// `scan_sk` and `spend_pk`; the spend secret key is not used.
    fn scan(
        &self,
        tweak_data: &TweakData,
        labels: Option<&LabelRegistry>,
        k_max: usize,
    ) -> Vec<DetectedSpCoin>;
}

impl SilentPaymentScan for SilentPaymentKeys {
    fn scan(
        &self,
        tweak_data: &TweakData,
        labels: Option<&LabelRegistry>,
        k_max: usize,
    ) -> Vec<DetectedSpCoin> {
        scan_from_tweaks(self.scan_sk(), self.spend_pk(), tweak_data, labels, k_max)
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::derive_onetime_sk;
    use super::*;
    use hex_literal::hex;

    // ─── TV1 pinned bytes (CHIP-0057 test vector 1) ────────────────────────

    const TV1_SCAN_SK: [u8; 32] =
        hex!("132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6");
    const TV1_SPEND_SK: [u8; 32] =
        hex!("53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087");
    const TV1_SPEND_PK: [u8; 48] = hex!(
        "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
        "f6f31e22e0831ba5088c952625a75c74"
    );
    const TV1_A_SUM: [u8; 48] = hex!(
        "8d9a5ed9c9b1a58476b07262007c636d775f2a33f0533737f3b3b0eaf99a8c0c"
        "51b3f2d87dc03a657e07f1828ab760fa"
    );
    const TV1_INPUT_HASH: [u8; 32] =
        hex!("38a1c8379cceb0fbebfdf3016707e54a1c7e9d21afb9489b9cc58f6055cc9411");
    const TV1_COIN_ID: [u8; 32] =
        hex!("5d759d2d97c03b1f6fe0657e91d25f6b7dd1311d6023271a1bcd35978a94a175");
    const TV1_PUZZLE_HASH: [u8; 32] =
        hex!("23adba149dd9000d65e0f8e21b6975364cbe89a63caf56533df4b7664c21fbf5");
    const TV1_ONETIME_SK: [u8; 32] =
        hex!("3c399c61ae130724903b3b650e936ff042b7646764289a33519e17100a89db37");
    const TV1_T0: [u8; 32] =
        hex!("5c560301c50fa309ad43d0f82cd1af143f6e3769659c80e8c14a072331582ab1");

    // ─── TV4 pinned bytes (CHIP-0057 test vector 4 — multi-input) ──────────

    const TV4_A_SUM: [u8; 48] = hex!(
        "a223ab27f801044cd98c8314014b8073347b0e5aae43c69b78b5ca2a562ee9f7"
        "99b8efad179b34da1b306ca4d62bad40"
    );
    const TV4_INPUT_HASH: [u8; 32] =
        hex!("3f1071552b7f2f5e49b68166cb204f0a1b6a23b0c30a28bcba59a9c3f766e166");
    const TV4_COIN_ID: [u8; 32] =
        hex!("209bb03a4cd165785e6149bc6dcb27e35829006f02ec927ab5a20521fd27d21a");
    const TV4_PUZZLE_HASH: [u8; 32] =
        hex!("5d7fc7d7447c746cfb400e801a169fc7bfd1c13e03bc7866e6b743860a53ac6b");
    const TV4_ONETIME_SK: [u8; 32] =
        hex!("6ccc3e13145fd561e438d1bb82954cebb63cfa9577ea15404987aa8e0f309399");

    // ─── Helpers ───────────────────────────────────────────────────────────

    fn sk(bytes: [u8; 32]) -> SecretKey {
        SecretKey::from_bytes(&bytes).expect("test vector secret key")
    }

    fn pk(bytes: [u8; 48]) -> PublicKey {
        PublicKey::from_bytes(&bytes).expect("test vector public key")
    }

    fn tweak_point_from(a_sum: [u8; 48], input_hash: [u8; 32]) -> PublicKey {
        let mut point = pk(a_sum);
        point.scalar_multiply(&input_hash);
        point
    }

    // ─── Tests ─────────────────────────────────────────────────────────────

    /// TV1: unlabeled detection at k=0 with TV1's pinned
    /// `shared_secret` → `t_0` → `onetime_pk` → `puzzle_hash` → `onetime_sk` chain.
    #[test]
    fn tv1_scan_detects_unlabeled_k0() {
        let data = TweakData {
            tweak_points: vec![tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH)],
            outputs: vec![OutputMeta {
                puzzle_hash: TV1_PUZZLE_HASH.into(),
                coin_id: TV1_COIN_ID.into(),
                amount: 1000,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            None,
            K_MAX_DEFAULT,
        );

        assert_eq!(detections.len(), 1, "expected exactly 1 TV1 detection");
        let detection = &detections[0];
        assert_eq!(detection.k, 0);
        assert!(detection.label.is_none(), "TV1 is unlabeled");
        assert_eq!(detection.puzzle_hash, Bytes32::from(TV1_PUZZLE_HASH));
        assert_eq!(detection.coin_id, Bytes32::from(TV1_COIN_ID));
        assert_eq!(detection.amount, 1000);
        // The detection carries t_0; the one-time key needs the spend key.
        assert_eq!(detection.tweak.to_bytes(), TV1_T0, "TV1 t_0 mismatch");
        assert_eq!(
            detection.onetime_sk(&sk(TV1_SPEND_SK)).to_bytes(),
            TV1_ONETIME_SK,
            "TV1 onetime_sk mismatch"
        );
    }

    /// TV4: multi-input aggregation. From the
    /// scanner's perspective the only difference vs TV1 is that the
    /// `A_sum` and `input_hash` are aggregated on the sender/indexer side
    /// — the scanner just gets a `tweak_point`. Tests that the scanner
    /// works for any `A_sum` + `input_hash` combination, not just the TV1
    /// single-input one.
    #[test]
    fn tv4_scan_detects_multi_input_aggregation() {
        // TV4 uses the same mnemonic / scan_sk / spend_sk / spend_pk as TV1.
        let data = TweakData {
            tweak_points: vec![tweak_point_from(TV4_A_SUM, TV4_INPUT_HASH)],
            outputs: vec![OutputMeta {
                puzzle_hash: TV4_PUZZLE_HASH.into(),
                coin_id: TV4_COIN_ID.into(),
                amount: 2000,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            None,
            K_MAX_DEFAULT,
        );

        assert_eq!(detections.len(), 1, "expected exactly 1 TV4 detection");
        let detection = &detections[0];
        assert_eq!(detection.k, 0);
        assert!(detection.label.is_none());
        assert_eq!(detection.puzzle_hash, Bytes32::from(TV4_PUZZLE_HASH));
        assert_eq!(
            detection.onetime_sk(&sk(TV1_SPEND_SK)).to_bytes(),
            TV4_ONETIME_SK,
            "TV4 onetime_sk mismatch"
        );
    }

    // ─── TV3 pinned bytes (CHIP-0057 test vector 3 — labeled, m = 1) ───────

    const TV3_INPUT_HASH: [u8; 32] =
        hex!("58a1875602949aa6bfaf9cb4837957e7175ffb0b14422dbc8d371799f98e66f5");
    const TV3_COIN_ID: [u8; 32] =
        hex!("4504f59ea184be18924f95244649287382ec6cdc13f333a8990f648c803a6dac");
    const TV3_PUZZLE_HASH: [u8; 32] =
        hex!("ba271d218d487e8e5dc994a09a8580e1e8a0559a615bd5805cff11b5a343441c");
    const TV3_LABELED_ONETIME_SK: [u8; 32] =
        hex!("58fc619583ff32e8e6e5cbe8587f4e1a395a04d538b132e5787d634cb64852dc");
    const TV3_T0: [u8; 32] =
        hex!("301e842ace534f7de854dcc5a48a656d7e9a6d8b8f93db9fb8277f4d1889bdf1");
    const TV3_LABEL_SCALAR: [u8; 32] =
        hex!("48fa440acca87f501b9984b5d23327d0b7766a4baa913dfb3001d412c48ce465");

    /// CHIP §459 identity-element guard: a `TweakData` containing
    /// `PublicKey::default()` (identity element) is skipped silently — no
    /// panic, no detections. Without this guard, the predictable shared
    /// secret derived from the identity element would enable false-positive
    /// detections at attacker-supplied puzzle hashes.
    #[test]
    fn identity_tweak_point_skipped() {
        // PublicKey::default() is the BLS12-381 G1 identity element.
        let identity = PublicKey::default();
        assert!(
            identity.is_inf(),
            "PublicKey::default() must be inf — sanity"
        );

        let data = TweakData {
            tweak_points: vec![identity],
            outputs: vec![OutputMeta {
                puzzle_hash: TV1_PUZZLE_HASH.into(),
                coin_id: TV1_COIN_ID.into(),
                amount: 1,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            None,
            K_MAX_DEFAULT,
        );

        assert!(
            detections.is_empty(),
            "identity-element tweak_point must produce no detections"
        );
    }

    /// TV3: labeled detection at k=0 with m=1.
    ///
    /// TV3 uses the same scan/spend keys as TV1. The labeled detection works
    /// by registering m=1 in the `LabelRegistry`; the scanner derives the
    /// labeled candidate `puzzle_hash_for_pk(candidate_pk + label_pk)` and
    /// matches `TV3_PUZZLE_HASH` byte-for-byte. The pinned
    /// `TV3_LABELED_ONETIME_SK = base_onetime_sk + label_scalar` confirms the
    /// labeled key-derivation chain.
    #[test]
    fn tv3_scan_detects_labeled_k0() {
        let mut labels = LabelRegistry::new();
        labels.register(&sk(TV1_SCAN_SK), 1);

        let data = TweakData {
            tweak_points: vec![tweak_point_from(TV1_A_SUM, TV3_INPUT_HASH)],
            outputs: vec![OutputMeta {
                puzzle_hash: TV3_PUZZLE_HASH.into(),
                coin_id: TV3_COIN_ID.into(),
                amount: 500,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            Some(&labels),
            K_MAX_DEFAULT,
        );

        assert_eq!(
            detections.len(),
            1,
            "expected exactly 1 TV3 labeled detection"
        );
        let detection = &detections[0];
        assert_eq!(detection.k, 0);
        assert_eq!(detection.label, Some(1), "TV3 is m=1");
        assert_eq!(detection.puzzle_hash, Bytes32::from(TV3_PUZZLE_HASH));
        // The combined tweak is (t_0 + label_scalar) mod r, so the signer needs
        // only the spend key (CHIP-0057 "Spending").
        let expected_tweak =
            ScalarField::from_bytes_raw(TV3_T0).add(&ScalarField::from_bytes_raw(TV3_LABEL_SCALAR));
        assert_eq!(detection.tweak.to_bytes(), expected_tweak.to_bytes());
        assert_eq!(
            detection.onetime_sk(&sk(TV1_SPEND_SK)).to_bytes(),
            TV3_LABELED_ONETIME_SK,
            "TV3 labeled onetime_sk mismatch"
        );
    }

    /// Bespoke `k = 1` vector.
    ///
    /// All CHIP TVs hit `k = 0`, so a naive `ser32(k) = k.to_le_bytes()`
    /// implementation would pass them all. This test pins a `k = 1` detection
    /// so a little-endian regression is caught.
    ///
    /// Construction (in-test derivation path):
    /// compute the `k = 1` expected `puzzle_hash` from TV1's `shared_secret`
    /// using the SDK's own protocol primitives, then build a `TweakData`
    /// carrying that `puzzle_hash` plus TV1's `k = 0` `puzzle_hash` (to keep
    /// the k-termination rule from firing at k=0). The asymmetry between
    /// `k = 0` (which any impl gets right) and `k = 1` (which only the
    /// correct big-endian `ser32` impl gets right) catches endianness
    /// regressions: under a little-endian `ser32`, the in-test
    /// `derive_output_tweak` would compute a different `t_1` and the
    /// pre-computed `expected_ph` would NOT match what the scanner finds
    /// for `k = 1`. Note that the scanner uses the same primitive, so a
    /// regression in `derive_output_tweak` would propagate to both sides;
    /// the residual guarantee is that the scanner's k=1 detection at the
    /// derived puzzle hash works at all, which exercises the full
    /// `ser32 → onetime_pk → puzzle_hash → onetime_sk` chain at `k = 1`.
    #[test]
    fn bespoke_k1_detection() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend = sk(TV1_SPEND_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);

        // Compute the expected k=1 puzzle_hash and onetime_sk using the SDK's
        // own protocol primitives.
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);
        let t1 = derive_output_tweak(&shared_secret, 1);
        let expected_onetime_pk = derive_onetime_pk(&b_spend_pub, &t1);
        let expected_ph_k1 = puzzle_hash_for_pk(&expected_onetime_pk);
        let expected_secret_k1 = derive_onetime_sk(&b_spend, &t1);

        let data = TweakData {
            tweak_points: vec![tp],
            outputs: vec![
                // k=0 output to satisfy the k-termination rule (without it
                // the loop breaks at k=0 with no match before reaching k=1).
                OutputMeta {
                    puzzle_hash: TV1_PUZZLE_HASH.into(),
                    coin_id: TV1_COIN_ID.into(),
                    amount: 100,
                    parent_coin_id: [0u8; 32].into(),
                },
                // k=1 output we're testing for.
                OutputMeta {
                    puzzle_hash: expected_ph_k1,
                    coin_id: hex!(
                        "00000000000000000000000000000000000000000000000000000000000000aa"
                    )
                    .into(),
                    amount: 200,
                    parent_coin_id: [0u8; 32].into(),
                },
            ],
        };

        let detections = scan_from_tweaks(&b_scan, &b_spend_pub, &data, None, K_MAX_DEFAULT);

        assert_eq!(detections.len(), 2, "expected k=0 + k=1 detection");
        let mut sorted = detections.clone();
        sorted.sort_by_key(|d| d.k);
        assert_eq!(sorted[0].k, 0);
        assert!(sorted[0].label.is_none());
        assert_eq!(
            sorted[1].k, 1,
            "k=1 must be detected — catches ser32 LE regression"
        );
        assert!(sorted[1].label.is_none());
        assert_eq!(sorted[1].puzzle_hash, expected_ph_k1);
        assert_eq!(
            sorted[1].onetime_sk(&b_spend).to_bytes(),
            expected_secret_k1.to_bytes(),
            "k=1 onetime_sk must equal (b_spend + t_1) mod r"
        );
    }

    /// Labeled k-termination rule: the k loop must NOT break after
    /// an unlabeled match at k=0 if a labeled candidate matches at k=1.
    /// Without this rule, labeled outputs that follow unlabeled outputs in
    /// the same spend group are silently missed.
    #[test]
    fn labeled_k_termination_rule() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);

        let mut labels = LabelRegistry::new();
        labels.register(&b_scan, 1);

        // Build the labeled puzzle_hash at k=1: candidate_pk_at_k1 + label_pk(m=1).
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);
        let t1 = derive_output_tweak(&shared_secret, 1);
        let candidate_pk_k1 = derive_onetime_pk(&b_spend_pub, &t1);
        let (_, label_pk_m1) = generate_label(&b_scan, 1);
        let labeled_pk_k1 = candidate_pk_k1 + &label_pk_m1;
        let labeled_hash_k1 = puzzle_hash_for_pk(&labeled_pk_k1);

        let data = TweakData {
            tweak_points: vec![tp],
            outputs: vec![
                // Unlabeled at k=0 (TV1's pinned PH).
                OutputMeta {
                    puzzle_hash: TV1_PUZZLE_HASH.into(),
                    coin_id: TV1_COIN_ID.into(),
                    amount: 100,
                    parent_coin_id: [0u8; 32].into(),
                },
                // Labeled at k=1 with m=1.
                OutputMeta {
                    puzzle_hash: labeled_hash_k1,
                    coin_id: hex!(
                        "00000000000000000000000000000000000000000000000000000000000000bb"
                    )
                    .into(),
                    amount: 200,
                    parent_coin_id: [0u8; 32].into(),
                },
            ],
        };

        let detections =
            scan_from_tweaks(&b_scan, &b_spend_pub, &data, Some(&labels), K_MAX_DEFAULT);

        assert_eq!(
            detections.len(),
            2,
            "expected both unlabeled-k0 + labeled-k1"
        );
        let mut sorted = detections.clone();
        sorted.sort_by_key(|d| d.k);
        assert_eq!(sorted[0].k, 0);
        assert!(sorted[0].label.is_none(), "k=0 is unlabeled");
        assert_eq!(sorted[1].k, 1);
        assert_eq!(sorted[1].label, Some(1), "k=1 is m=1");
    }

    /// When both an unlabeled candidate AND a labeled candidate would match at
    /// the same k, the scanner emits the unlabeled
    /// detection (`label = None`). The labeled branch is `if !found { ... }`-
    /// guarded so it only runs when the unlabeled branch missed. Mirrors
    /// `sp-client/scanner.rs::test_scan_block_unlabeled_preferred`.
    ///
    /// We verify this property indirectly: with `m = 1` registered AND TV1's
    /// unlabeled output present, the scanner emits exactly ONE detection (the
    /// unlabeled one). If the labeled branch were not guarded by `if !found`,
    /// the labeled branch could iterate `label_map.iter()` after the unlabeled
    /// match and produce extra detections; the assertion `detections.len() == 1`
    /// + `label.is_none()` catches that regression.
    #[test]
    fn unlabeled_preferred_over_labeled_at_same_k() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);

        let mut labels = LabelRegistry::new();
        labels.register(&b_scan, 1);

        let data = TweakData {
            tweak_points: vec![tp],
            outputs: vec![OutputMeta {
                puzzle_hash: TV1_PUZZLE_HASH.into(),
                coin_id: TV1_COIN_ID.into(),
                amount: 100,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let detections =
            scan_from_tweaks(&b_scan, &b_spend_pub, &data, Some(&labels), K_MAX_DEFAULT);

        assert_eq!(
            detections.len(),
            1,
            "exactly one detection — unlabeled wins"
        );
        assert!(
            detections[0].label.is_none(),
            "unlabeled preferred over labeled at same k"
        );
    }

    /// CHIP §416 DOS guard: a `TweakData` with many forged matches
    /// (one per k from 0..N) where N >> `k_max` must terminate at `k_max` and
    /// produce at most `k_max` detections.
    ///
    /// Construction: for each k ∈ [0, 9999], compute the `puzzle_hash` the
    /// scanner WILL derive at that k for TV1's keys + `tweak_point`. Stuff all
    /// 10,000 into the `OutputMeta` list. The scanner finds a "match" at every
    /// k, so its `if !found { break; }` never fires from a miss — only the
    /// `k_max` cap can stop it.
    #[test]
    fn dos_guard_caps_at_k_max() {
        const N_FORGED: u32 = 10_000;
        const K_MAX_TEST: usize = 32;

        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);

        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);

        let outputs: Vec<OutputMeta> = (0..N_FORGED)
            .map(|k| {
                let tweak = derive_output_tweak(&shared_secret, k);
                let onetime_pk = derive_onetime_pk(&b_spend_pub, &tweak);
                let ph = puzzle_hash_for_pk(&onetime_pk);
                OutputMeta {
                    puzzle_hash: ph,
                    coin_id: [0u8; 32].into(),
                    amount: 1,
                    parent_coin_id: [0u8; 32].into(),
                }
            })
            .collect();

        let data = TweakData {
            tweak_points: vec![tp],
            outputs,
        };

        let detections = scan_from_tweaks(&b_scan, &b_spend_pub, &data, None, K_MAX_TEST);

        assert!(
            detections.len() <= K_MAX_TEST,
            "DOS guard failed: got {} detections, expected <= {K_MAX_TEST}",
            detections.len()
        );
    }

    /// Verify the bundled `SilentPaymentScan::scan` method on
    /// `SilentPaymentKeys` produces byte-for-byte identical results to the
    /// free function `scan_from_tweaks`. Demonstrates the two API surfaces
    /// coexist — the trait method exists for callers who hold a bundled
    /// `SilentPaymentKeys`, while the free function is the entry point for
    /// watch-only scanners that hold no spend secret key.
    #[test]
    fn silent_payment_keys_scan_method_matches_free_fn_tv1() {
        use chia_sdk_utils::silent_payments::SilentPaymentKeys;

        let data = TweakData {
            tweak_points: vec![tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH)],
            outputs: vec![OutputMeta {
                puzzle_hash: TV1_PUZZLE_HASH.into(),
                coin_id: TV1_COIN_ID.into(),
                amount: 1000,
                parent_coin_id: [0u8; 32].into(),
            }],
        };

        let free_fn_result = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            None,
            K_MAX_DEFAULT,
        );

        let keys = SilentPaymentKeys::from_secret_keys(sk(TV1_SCAN_SK), sk(TV1_SPEND_SK));
        let method_result = keys.scan(&data, None, K_MAX_DEFAULT);

        assert_eq!(free_fn_result.len(), method_result.len());
        assert_eq!(free_fn_result.len(), 1, "TV1 sanity");
        assert_eq!(free_fn_result[0].coin_id, method_result[0].coin_id);
        assert_eq!(free_fn_result[0].puzzle_hash, method_result[0].puzzle_hash);
        assert_eq!(free_fn_result[0].k, method_result[0].k);
        assert_eq!(
            free_fn_result[0].tweak.to_bytes(),
            method_result[0].tweak.to_bytes()
        );
        assert_eq!(free_fn_result[0].label, method_result[0].label);
    }

    /// CHIP-0057 "Edge Cases", zero scalars: a zero tweak `t_k` stops the scan of
    /// the group, even when later indices would match. A hash that reduces to
    /// zero cannot be produced on demand, so the tweak sequence is injected.
    #[test]
    fn zero_tweak_stops_scanning_the_group() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);

        // Outputs at k = 0, 1 and 2.
        let outputs: Vec<OutputMeta> = (0..3u8)
            .map(|k| OutputMeta {
                puzzle_hash: puzzle_hash_for_pk(&derive_onetime_pk(
                    &b_spend_pub,
                    &derive_output_tweak(&shared_secret, u32::from(k)),
                )),
                coin_id: [k; 32].into(),
                amount: 1,
                parent_coin_id: [0u8; 32].into(),
            })
            .collect();
        let data = TweakData {
            tweak_points: vec![tp],
            outputs,
        };
        let outputs = index_outputs(&data.outputs);

        // With the real tweaks all three outputs are found.
        let mut detected = Vec::new();
        scan_group(
            |k| derive_output_tweak(&shared_secret, k),
            &b_scan,
            &b_spend_pub,
            &outputs,
            &[],
            2400,
            &mut detected,
        );
        assert_eq!(detected.len(), 3);

        // With t_1 == 0 the scan stops after k = 0.
        let mut detected = Vec::new();
        scan_group(
            |k| {
                if k == 1 {
                    ScalarField::from_bytes_raw([0u8; 32])
                } else {
                    derive_output_tweak(&shared_secret, k)
                }
            },
            &b_scan,
            &b_spend_pub,
            &outputs,
            &[],
            2400,
            &mut detected,
        );
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].k, 0);
    }

    /// A watch-only scanner holds `b_scan` and `B_spend` only. Its detections
    /// carry the combined tweak, which is handed (here as 32 bytes) to a signer
    /// that holds `b_spend`; the signer's one-time key controls the detected
    /// puzzle hash. Covers an unlabeled and a labeled output (CHIP-0057
    /// "Spending": the signer needs only `b_spend`).
    #[test]
    fn watch_only_detection_is_completed_by_the_spend_key() {
        let mut labels = LabelRegistry::new();
        labels.register(&sk(TV1_SCAN_SK), 1);

        let data = TweakData {
            tweak_points: vec![
                tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH),
                tweak_point_from(TV1_A_SUM, TV3_INPUT_HASH),
            ],
            outputs: vec![
                OutputMeta {
                    puzzle_hash: TV1_PUZZLE_HASH.into(),
                    coin_id: TV1_COIN_ID.into(),
                    amount: 1000,
                    parent_coin_id: [1u8; 32].into(),
                },
                OutputMeta {
                    puzzle_hash: TV3_PUZZLE_HASH.into(),
                    coin_id: TV3_COIN_ID.into(),
                    amount: 500,
                    parent_coin_id: [3u8; 32].into(),
                },
            ],
        };

        // Scanner side: no spend secret key in sight.
        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            Some(&labels),
            K_MAX_DEFAULT,
        );
        assert_eq!(detections.len(), 2);
        let handed_over: Vec<([u8; 32], Bytes32)> = detections
            .iter()
            .map(|d| (d.tweak.to_bytes(), d.puzzle_hash))
            .collect();
        assert_eq!(detections[0].coin().puzzle_hash, detections[0].puzzle_hash);
        assert_eq!(detections[0].coin().amount, 1000);
        assert_eq!(detections[0].coin().parent_coin_info, [1u8; 32].into());

        // Signer side: the spend secret key and the tweak are enough.
        let b_spend = sk(TV1_SPEND_SK);
        let expected = [TV1_ONETIME_SK, TV3_LABELED_ONETIME_SK];
        for ((tweak, puzzle_hash), expected_sk) in handed_over.into_iter().zip(expected) {
            let onetime_sk = derive_onetime_sk(&b_spend, &ScalarField::from_bytes_unsigned(tweak));
            assert_eq!(onetime_sk.to_bytes(), expected_sk);
            assert_eq!(puzzle_hash_for_pk(&onetime_sk.public_key()), puzzle_hash);
        }
    }

    /// CHIP-0057 "Tweak Points": a tweak point supplied by another party must
    /// be a non-identity element of the prime-order subgroup before the scan
    /// key is multiplied by it. A point on the curve but outside the subgroup
    /// is skipped, and does not stop the valid tweak point after it from being
    /// scanned.
    #[test]
    fn tweak_point_outside_the_subgroup_is_skipped() {
        // Find a compressed encoding that decodes to a curve point outside G1.
        // The cofactor of G1 is large, so almost every curve point qualifies.
        let off_subgroup = (1..=u8::MAX)
            .find_map(|x| {
                let mut bytes = [0u8; 48];
                bytes[0] = 0x80;
                bytes[47] = x;
                PublicKey::from_bytes_unchecked(&bytes)
                    .ok()
                    .filter(|p| !p.is_valid())
            })
            .expect("a curve point outside the prime-order subgroup");
        assert!(!off_subgroup.is_inf());

        let output = OutputMeta {
            puzzle_hash: TV1_PUZZLE_HASH.into(),
            coin_id: TV1_COIN_ID.into(),
            amount: 1000,
            parent_coin_id: [0u8; 32].into(),
        };

        // Any output the invalid point would "pay" must not be reported: build
        // the puzzle hash a scanner would derive from it if it did not check.
        let leaked_secret = compute_shared_secret_from_tweak(&sk(TV1_SCAN_SK), &off_subgroup);
        let leaked_ph = puzzle_hash_for_pk(&derive_onetime_pk(
            &pk(TV1_SPEND_PK),
            &derive_output_tweak(&leaked_secret, 0),
        ));

        let data = TweakData {
            tweak_points: vec![off_subgroup, tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH)],
            outputs: vec![
                OutputMeta {
                    puzzle_hash: leaked_ph,
                    coin_id: [9u8; 32].into(),
                    amount: 1,
                    parent_coin_id: [0u8; 32].into(),
                },
                output,
            ],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            None,
            K_MAX_DEFAULT,
        );
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].puzzle_hash, Bytes32::from(TV1_PUZZLE_HASH));
    }

    /// CHIP-0057 "Outputs Sharing a Puzzle Hash": two output coins with the
    /// same one-time puzzle hash (different parents and amounts) are both
    /// reported, with the same `k` and tweak, and scanning continues to the
    /// next index.
    #[test]
    fn all_coins_sharing_a_puzzle_hash_are_reported() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);
        let ph_k1 = puzzle_hash_for_pk(&derive_onetime_pk(
            &b_spend_pub,
            &derive_output_tweak(&shared_secret, 1),
        ));

        let data = TweakData {
            tweak_points: vec![tp],
            outputs: vec![
                OutputMeta {
                    puzzle_hash: TV1_PUZZLE_HASH.into(),
                    coin_id: [1u8; 32].into(),
                    amount: 100,
                    parent_coin_id: [0xaau8; 32].into(),
                },
                OutputMeta {
                    puzzle_hash: [0x99u8; 32].into(),
                    coin_id: [9u8; 32].into(),
                    amount: 5,
                    parent_coin_id: [0xaau8; 32].into(),
                },
                OutputMeta {
                    puzzle_hash: TV1_PUZZLE_HASH.into(),
                    coin_id: [2u8; 32].into(),
                    amount: 200,
                    parent_coin_id: [0xbbu8; 32].into(),
                },
                OutputMeta {
                    puzzle_hash: ph_k1,
                    coin_id: [3u8; 32].into(),
                    amount: 300,
                    parent_coin_id: [0xaau8; 32].into(),
                },
            ],
        };

        let detections = scan_from_tweaks(&b_scan, &b_spend_pub, &data, None, K_MAX_DEFAULT);

        let found: Vec<(Bytes32, u64, u32)> = detections
            .iter()
            .map(|d| (d.coin_id, d.amount, d.k))
            .collect();
        assert_eq!(
            found,
            vec![
                ([1u8; 32].into(), 100, 0),
                ([2u8; 32].into(), 200, 0),
                ([3u8; 32].into(), 300, 1),
            ]
        );
        assert_eq!(detections[0].tweak, detections[1].tweak);
        assert_eq!(detections[0].tweak.to_bytes(), TV1_T0);
    }

    /// The same holds for a labeled output: every coin with the labeled
    /// puzzle hash is reported under that label.
    #[test]
    fn all_coins_sharing_a_labeled_puzzle_hash_are_reported() {
        let mut labels = LabelRegistry::new();
        labels.register(&sk(TV1_SCAN_SK), 1);

        let coin = |id: u8, amount: u64| OutputMeta {
            puzzle_hash: TV3_PUZZLE_HASH.into(),
            coin_id: [id; 32].into(),
            amount,
            parent_coin_id: [id; 32].into(),
        };
        let data = TweakData {
            tweak_points: vec![tweak_point_from(TV1_A_SUM, TV3_INPUT_HASH)],
            outputs: vec![coin(1, 10), coin(2, 20), coin(3, 30)],
        };

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &data,
            Some(&labels),
            K_MAX_DEFAULT,
        );
        assert_eq!(detections.len(), 3);
        for (detection, amount) in detections.iter().zip([10, 20, 30]) {
            assert_eq!(detection.amount, amount);
            assert_eq!(detection.k, 0);
            assert_eq!(detection.label, Some(1));
            assert_eq!(
                detection.onetime_sk(&sk(TV1_SPEND_SK)).to_bytes(),
                TV3_LABELED_ONETIME_SK
            );
        }
    }

    // ─── CHIP-0057 Test Vector 7: change output (label m = 0) ──────────────

    const TV7_COIN_ID: [u8; 32] =
        hex!("10f36babd97f3da5027238f336ac15a0f12dfb10bc77191711de7eafba964c9f");
    const TV7_INPUT_HASH: [u8; 32] =
        hex!("620c82f4fd9faf8a41d7ed8025dbda3857797c0bc5f31f4c34e460ddc4a89e79");
    const TV7_PUZZLE_HASH: [u8; 32] =
        hex!("6e0d9f029ea7e8129bf4839d7e805dac5da5b9af5f51fe0374547ac24f8f5257");
    const TV7_UNLABELED_CANDIDATE: [u8; 32] =
        hex!("980d14e591ef9db6d449eae11c7c43b3f753f07c79da105a06f27f75a2384dc1");
    const TV7_ONETIME_SK: [u8; 32] =
        hex!("254f5ce70b4870c4a9b2811bb36b0e79910a88b839a1c6771ab2d222cb0fd42a");

    fn tv7_data() -> TweakData {
        TweakData {
            tweak_points: vec![tweak_point_from(TV1_A_SUM, TV7_INPUT_HASH)],
            outputs: vec![OutputMeta {
                puzzle_hash: TV7_PUZZLE_HASH.into(),
                coin_id: TV7_COIN_ID.into(),
                amount: 700,
                parent_coin_id: [0u8; 32].into(),
            }],
        }
    }

    /// The scanner always checks the change label, so the TV7 output is found
    /// and reported as label 0 without the caller registering anything.
    #[test]
    fn tv7_change_output_is_found_without_registering_labels() {
        for labels in [None, Some(&LabelRegistry::new())] {
            let detections = scan_from_tweaks(
                &sk(TV1_SCAN_SK),
                &pk(TV1_SPEND_PK),
                &tv7_data(),
                labels,
                K_MAX_DEFAULT,
            );
            assert_eq!(detections.len(), 1);
            assert_eq!(detections[0].k, 0);
            assert_eq!(detections[0].label, Some(0));
            assert_eq!(detections[0].puzzle_hash, Bytes32::from(TV7_PUZZLE_HASH));
            assert_eq!(
                detections[0].onetime_sk(&sk(TV1_SPEND_SK)).to_bytes(),
                TV7_ONETIME_SK
            );
        }
    }

    /// Registering label 0 explicitly, alongside another label, changes
    /// nothing: the output is reported once, as label 0.
    #[test]
    fn tv7_change_output_with_registered_labels() {
        let mut labels = LabelRegistry::new();
        labels.register(&sk(TV1_SCAN_SK), 1);
        labels.register(&sk(TV1_SCAN_SK), 0);

        let detections = scan_from_tweaks(
            &sk(TV1_SCAN_SK),
            &pk(TV1_SPEND_PK),
            &tv7_data(),
            Some(&labels),
            K_MAX_DEFAULT,
        );
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].label, Some(0));
    }

    /// TV7 verification: with an empty label set the output is not found,
    /// because the unlabeled candidate puzzle hash at k = 0 is not on chain.
    /// `scan_from_tweaks` never scans with an empty label set, so this goes
    /// through the per-group loop directly.
    #[test]
    fn tv7_empty_label_set_does_not_find_the_change_output() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let data = tv7_data();
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &data.tweak_points[0]);

        let unlabeled = puzzle_hash_for_pk(&derive_onetime_pk(
            &b_spend_pub,
            &derive_output_tweak(&shared_secret, 0),
        ));
        assert_eq!(unlabeled, Bytes32::from(TV7_UNLABELED_CANDIDATE));

        let outputs = index_outputs(&data.outputs);
        let mut detected = Vec::new();
        scan_group(
            |k| derive_output_tweak(&shared_secret, k),
            &b_scan,
            &b_spend_pub,
            &outputs,
            &[],
            2400,
            &mut detected,
        );
        assert!(detected.is_empty());
    }

    /// The label set always starts with the change label, followed by the
    /// registered labels in ascending order, without duplicating label 0.
    #[test]
    fn label_set_always_contains_the_change_label_first() {
        let b_scan = sk(TV1_SCAN_SK);
        let order = |labels: Option<&LabelRegistry>| -> Vec<u32> {
            label_set(&b_scan, labels).iter().map(|(m, _)| *m).collect()
        };
        assert_eq!(order(None), vec![0]);

        let mut labels = LabelRegistry::new();
        for m in [5, 0, 2] {
            labels.register(&b_scan, m);
        }
        assert_eq!(order(Some(&labels)), vec![0, 2, 5]);
    }

    /// CHIP-0057 "Kmax": a scanner detects outputs at every index below
    /// `K_max` and stops when `k` reaches it, whatever cap the caller passes.
    #[test]
    fn scan_stops_at_k_max() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);

        // Outputs at every index from 0 through K_max (one more than allowed).
        let outputs: Vec<OutputMeta> = (0..=K_MAX)
            .map(|k| OutputMeta {
                puzzle_hash: puzzle_hash_for_pk(&derive_onetime_pk(
                    &b_spend_pub,
                    &derive_output_tweak(&shared_secret, k),
                )),
                coin_id: Bytes32::from([0u8; 32]),
                amount: u64::from(k),
                parent_coin_id: [0u8; 32].into(),
            })
            .collect();
        let data = TweakData {
            tweak_points: vec![tp],
            outputs,
        };

        for k_max in [K_MAX_DEFAULT, K_MAX_DEFAULT + 1, usize::MAX] {
            let detections = scan_from_tweaks(&b_scan, &b_spend_pub, &data, None, k_max);
            assert_eq!(detections.len(), K_MAX as usize);
            assert_eq!(detections.last().unwrap().k, K_MAX - 1);
        }
    }

    /// CHIP-0057 "Required Behaviors": a matching output that wallet policy
    /// filters out still advances `k`.
    ///
    /// The scanner applies no policy: it reports every match and decides
    /// whether to continue from the match alone. A wallet filters the returned
    /// detections afterwards, so a dust output at k = 0 (here, 0 mojos) cannot
    /// hide the outputs at k = 1 and k = 2.
    #[test]
    fn filtered_match_still_advances_k() {
        let b_scan = sk(TV1_SCAN_SK);
        let b_spend_pub = pk(TV1_SPEND_PK);
        let tp = tweak_point_from(TV1_A_SUM, TV1_INPUT_HASH);
        let shared_secret = compute_shared_secret_from_tweak(&b_scan, &tp);

        let amounts = [0u64, 5_000, 1];
        let outputs: Vec<OutputMeta> = amounts
            .iter()
            .zip(0u32..)
            .map(|(&amount, k)| OutputMeta {
                puzzle_hash: puzzle_hash_for_pk(&derive_onetime_pk(
                    &b_spend_pub,
                    &derive_output_tweak(&shared_secret, k),
                )),
                coin_id: [u8::try_from(k).unwrap(); 32].into(),
                amount,
                parent_coin_id: [0u8; 32].into(),
            })
            .collect();
        let data = TweakData {
            tweak_points: vec![tp],
            outputs,
        };

        let detections = scan_from_tweaks(&b_scan, &b_spend_pub, &data, None, K_MAX_DEFAULT);
        assert_eq!(
            detections
                .iter()
                .map(|d| (d.k, d.amount))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 5_000), (2, 1)]
        );

        // The wallet's dust filter runs on the result.
        let kept: Vec<u32> = detections
            .iter()
            .filter(|d| d.amount >= 1_000)
            .map(|d| d.k)
            .collect();
        assert_eq!(kept, vec![1]);
    }
}
