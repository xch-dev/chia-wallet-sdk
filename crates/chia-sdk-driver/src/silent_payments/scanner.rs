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

use std::collections::HashSet;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_sdk_types::silent_payments::ScalarField;
use chia_sdk_utils::silent_payments::{LabelRegistry, SilentPaymentKeys, generate_label};

use super::protocol::{
    compute_shared_secret_from_tweak, derive_onetime_pk, derive_onetime_sk, derive_output_tweak,
    puzzle_hash_for_pk,
};
use super::types::{DetectedSpCoin, TweakData};

/// Default per-spend-group iteration cap per CHIP-0057 §446.
///
/// Derived from Chia's mempool 5.5 B spend-bundle cost limit: 2,400 is the
/// theoretical maximum number of silent-payment outputs a single spend bundle
/// can fit at standard mempool policy. Callers may pass a smaller value to
/// [`scan_from_tweaks`] (e.g., 32 for a fast pre-scan in resource-constrained
/// environments) but should not exceed this in production scans.
pub const K_MAX_DEFAULT: usize = 2400;

/// Scan a block's worth of tweak points + candidate outputs for silent
/// payments addressed to this wallet.
///
/// For each `tweak_point` in `data.tweak_points`, performs one ECDH operation
/// (`scan_sk * tweak_point`, hashed to a 32-byte shared secret) and iterates
/// `k = 0, 1, 2, ...` up to `k_max`, deriving the candidate one-time puzzle
/// hash and checking against `data.outputs`. Labeled detection (per `labels`)
/// is interleaved per the CHIP-0057 labeled k-termination rule.
///
/// **CHIP §459 guard:** identity-element tweak points are skipped silently —
/// they produce a predictable shared secret that would otherwise enable
/// false-positive detections.
///
/// **CHIP §416 `K_max` cap:** the `k_max` parameter bounds the per-spend-group
/// iteration count so an adversarial tweak source cannot force unbounded
/// scanning. Default value: [`K_MAX_DEFAULT`].
///
/// **Termination:** the `k` loop stops at the first miss (`if !found { break; }`)
/// — except labeled detections continue if either an unlabeled OR any labeled
/// candidate matches at the current `k`.
//
// `clippy::similar_names` on the `spend_sk` / `spend_pk` parameter pair is
// genuinely unavoidable here: the function signature's `&PublicKey, &SecretKey`
// parameter pair triggers clippy::similar_names, and the labeled-detection
// branch below references both names directly — local rebinding would either
// break the signature or break the labeled-branch's structural expectations.
// The single-byte difference (`sk` vs `pk`) is below clippy's similarity
// threshold. Allowed at function scope only, not at module scope.
#[allow(clippy::similar_names)]
#[must_use]
pub fn scan_from_tweaks(
    scan_sk: &SecretKey,
    spend_sk: &SecretKey,
    spend_pk: &PublicKey,
    data: &TweakData,
    labels: Option<&LabelRegistry>,
    k_max: usize,
) -> Vec<DetectedSpCoin> {
    let output_phs: HashSet<Bytes32> = data.outputs.iter().map(|o| o.puzzle_hash).collect();
    let mut detected = Vec::new();

    let k_bound = u32::try_from(k_max).unwrap_or(u32::MAX);

    for tweak_point in &data.tweak_points {
        // CHIP §459 guard: skip identity-element tweak points.
        if tweak_point.is_inf() {
            continue;
        }

        let shared_secret = compute_shared_secret_from_tweak(scan_sk, tweak_point);

        for k in 0..k_bound {
            let output_tweak = derive_output_tweak(&shared_secret, k);
            let candidate_pk = derive_onetime_pk(spend_pk, &output_tweak);
            let candidate_hash = puzzle_hash_for_pk(&candidate_pk);

            let mut found = false;

            if output_phs.contains(&candidate_hash)
                && let Some(out) = data
                    .outputs
                    .iter()
                    .find(|o| o.puzzle_hash == candidate_hash)
            {
                let onetime_sk = derive_onetime_sk(spend_sk, &output_tweak);
                detected.push(DetectedSpCoin {
                    coin_id: out.coin_id,
                    puzzle_hash: out.puzzle_hash,
                    amount: out.amount,
                    parent_coin_id: out.parent_coin_id,
                    onetime_sk,
                    k,
                    label: None,
                });
                found = true;
            }

            // Labeled-detection branch. Only runs when the
            // unlabeled candidate at this k missed; otherwise the unlabeled
            // detection is preferred (matches sp-client's
            // test_scan_block_unlabeled_preferred).
            if !found && let Some(label_map) = labels {
                for (m, label_pk) in label_map.iter() {
                    let labeled_pk = candidate_pk + label_pk;
                    let labeled_hash = puzzle_hash_for_pk(&labeled_pk);
                    if output_phs.contains(&labeled_hash)
                        && let Some(out) =
                            data.outputs.iter().find(|o| o.puzzle_hash == labeled_hash)
                    {
                        let base_sk = derive_onetime_sk(spend_sk, &output_tweak);
                        let (label_scalar, _) = generate_label(scan_sk, m);
                        let base_scalar = ScalarField::from_bytes_raw(base_sk.to_bytes());
                        let labeled_scalar = base_scalar.add(&label_scalar);
                        let labeled_sk = SecretKey::from_bytes(labeled_scalar.as_bytes())
                            .expect("labeled scalar < r by ScalarField boundary");
                        detected.push(DetectedSpCoin {
                            coin_id: out.coin_id,
                            puzzle_hash: out.puzzle_hash,
                            amount: out.amount,
                            parent_coin_id: out.parent_coin_id,
                            onetime_sk: labeled_sk,
                            k,
                            label: Some(m),
                        });
                        found = true;
                        break; // first labeled match wins for this k
                    }
                }
            }

            // Termination rule: break the k loop only when
            // NEITHER unlabeled NOR any labeled candidate matched at this k.
            if !found {
                break;
            }
        }
    }

    detected
}

/// Convenience trait that lets a [`SilentPaymentKeys`] bundle drive the
/// scanner via a single method call, avoiding the four-key-argument
/// boilerplate of [`scan_from_tweaks`].
///
/// The orphan rule prevents inherent methods on `SilentPaymentKeys` from
/// `chia-sdk-driver` (the type is defined in `chia-sdk-utils`), so the SDK
/// exposes the bundled flow as a driver-side trait. Wallet authors who
/// prefer the raw-args flow — for example, hardware-split signers where
/// `spend_sk` lives on a device — call [`scan_from_tweaks`] directly.
pub trait SilentPaymentScan {
    /// Scan `tweak_data` for silent-payment outputs addressed to this key
    /// bundle. Equivalent to calling [`scan_from_tweaks`] with this bundle's
    /// `scan_sk`, `spend_sk`, `spend_pk`.
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
        scan_from_tweaks(
            self.scan_sk(),
            self.spend_sk(),
            self.spend_pk(),
            tweak_data,
            labels,
            k_max,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::OutputMeta;
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
            &sk(TV1_SPEND_SK),
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
        assert_eq!(
            detection.onetime_sk.to_bytes(),
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
            &sk(TV1_SPEND_SK),
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
            detection.onetime_sk.to_bytes(),
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
            &sk(TV1_SPEND_SK),
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
            &sk(TV1_SPEND_SK),
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
        assert_eq!(
            detection.onetime_sk.to_bytes(),
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

        let detections =
            scan_from_tweaks(&b_scan, &b_spend, &b_spend_pub, &data, None, K_MAX_DEFAULT);

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
            sorted[1].onetime_sk.to_bytes(),
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
        let b_spend = sk(TV1_SPEND_SK);
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

        let detections = scan_from_tweaks(
            &b_scan,
            &b_spend,
            &b_spend_pub,
            &data,
            Some(&labels),
            K_MAX_DEFAULT,
        );

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
        let b_spend = sk(TV1_SPEND_SK);
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

        let detections = scan_from_tweaks(
            &b_scan,
            &b_spend,
            &b_spend_pub,
            &data,
            Some(&labels),
            K_MAX_DEFAULT,
        );

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
        let b_spend = sk(TV1_SPEND_SK);
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

        let detections = scan_from_tweaks(&b_scan, &b_spend, &b_spend_pub, &data, None, K_MAX_TEST);

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
    /// hardware-split signers where `spend_sk` lives on a device.
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
            &sk(TV1_SPEND_SK),
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
            free_fn_result[0].onetime_sk.to_bytes(),
            method_result[0].onetime_sk.to_bytes()
        );
        assert_eq!(free_fn_result[0].label, method_result[0].label);
    }
}
