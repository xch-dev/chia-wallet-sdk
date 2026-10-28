#![cfg(feature = "chip-0057")]

//! Walks the machine-readable test vectors of CHIP-0057.
//!
//! `tests/data/chip-0057/test_vectors.json` is an unmodified copy of
//! `assets/chip-0057/test_vectors.json` from the CHIPs repository. Every value
//! in the file is checked against what this SDK computes: the payments (both
//! as the sender derives them and as a scanner detects them), the labels, the
//! addresses, the address cases, and the key derivation.
//!
//! The file is the authority. If a value or an address case disagrees with the
//! SDK, the SDK is wrong, or the CHIP has to be corrected; the vendored file
//! is not edited to make a test pass. It is pinned by its sha256
//! ([`VECTORS_SHA256`]), checked before it is parsed, so a substituted or
//! edited copy fails every test.
//!
//! The payments of the Extension to CATs (Vectors 9 to 11) carry a wrapper per
//! input and output and the block's wrapper list. Their `puzzle_hash` is the
//! bare one-time puzzle hash; the coin's puzzle hash is `wrapped_puzzle_hash`.
//! The `settlement_payments` (Vectors 12 and 13) are walked the same way, and
//! `DeriveSettlementOutputs` reproduces their notarized payments, announcements
//! and the outputs of the party's spend group.

use std::collections::BTreeSet;

use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzle_types::standard::StandardArgs;
use chia_puzzle_types::{LineageProof, Memos};
use chia_sdk_driver::silent_payments::{
    K_MAX_DEFAULT, OtherPayment, OutputMeta, SettlementDerivation, SettlementGroupCoin,
    SettlementRecipient, SyntheticSecretKey, TweakData, Wrapper, aggregate_sender_sks,
    compute_input_hash, compute_shared_secret_from_tweak, compute_tweak_point,
    derive_one_time_puzzle_hash, derive_onetime_pk, derive_onetime_sk, derive_output_tweak,
    derive_settlement_outputs, eligible_spend, encoded_payment, puzzle_hash_for_pk,
    scan_from_tweaks, settlement_announcement, settlement_puzzle_hash, sort_wrappers,
    wrap_puzzle_hash,
};
use chia_sdk_driver::{Cat, CatInfo, CatSpend, SpendContext, SpendWithConditions, StandardLayer};
use chia_sdk_types::Conditions;
use chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH, ScalarField};
use chia_sdk_utils::silent_payments::{
    LabelRegistry, SP_ADDRESS_VERSION, SilentPaymentAddress, SilentPaymentKeys,
    SilentPaymentNetwork, generate_label,
};
use chia_sha2::Sha256;
use clvm_traits::ToClvm;
use clvmr::Allocator;
use clvmr::serde::{node_from_bytes, node_to_bytes};
use serde_json::Value;

const VECTORS: &str = include_str!("data/chip-0057/test_vectors.json");

/// sha256 of the CHIP's vectors file, identical in every copy (the CHIPs
/// repository, the reference implementation, and this SDK).
const VECTORS_SHA256: &str = "c4fc3a776dd1805c0e2ea5bef486d17c68e8742e067131968eb0794bc0f85fb8";

// ─── Reading the file ───────────────────────────────────────────────────────

/// The vendored file, parsed after its sha256 is checked against
/// [`VECTORS_SHA256`].
fn vectors() -> Value {
    let mut hasher = Sha256::new();
    hasher.update(VECTORS.as_bytes());
    let digest = hex::encode(hasher.finalize());
    assert_eq!(
        digest, VECTORS_SHA256,
        "tests/data/chip-0057/test_vectors.json is not the CHIP's vectors file: it has sha256 \
         {digest}"
    );
    serde_json::from_str(VECTORS).expect("test_vectors.json is valid JSON")
}

fn list<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` is not a list"))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is not a string"))
}

fn bytes<const N: usize>(value: &Value, key: &str) -> [u8; N] {
    let decoded =
        hex::decode(text(value, key)).unwrap_or_else(|e| panic!("`{key}` is not hex: {e}"));
    decoded
        .try_into()
        .unwrap_or_else(|v: Vec<u8>| panic!("`{key}` is {} bytes, expected {N}", v.len()))
}

fn sk(value: &Value, key: &str) -> SecretKey {
    SecretKey::from_bytes(&bytes(value, key))
        .unwrap_or_else(|e| panic!("`{key}` is not a secret key: {e}"))
}

fn pk(value: &Value, key: &str) -> PublicKey {
    PublicKey::from_bytes(&bytes(value, key))
        .unwrap_or_else(|e| panic!("`{key}` is not a G1 element: {e}"))
}

/// A label index: `null` for an unlabeled output.
fn label(value: &Value) -> Option<u32> {
    match &value["label"] {
        Value::Null => None,
        m => Some(u32::try_from(m.as_u64().expect("`label` is an integer")).unwrap()),
    }
}

fn index(value: &Value, key: &str) -> u32 {
    u32::try_from(value[key].as_u64().expect("an integer")).unwrap()
}

/// A wrapper in the file's shape `{asset_id, hidden_puzzle_hash | null}`;
/// `null` (or a missing key) is XCH.
fn wrapper(value: &Value) -> Option<Wrapper> {
    if value.is_null() {
        return None;
    }
    let asset_id = Bytes32::new(bytes(value, "asset_id"));
    Some(match &value["hidden_puzzle_hash"] {
        Value::Null => Wrapper::cat(asset_id),
        _ => Wrapper::revocable(asset_id, Bytes32::new(bytes(value, "hidden_puzzle_hash"))),
    })
}

/// The block's wrapper list of a payment; empty when the entry has none.
fn wrapper_list(payment: &Value) -> Vec<Wrapper> {
    match &payment["wrapper_list"] {
        Value::Null => Vec::new(),
        list => list
            .as_array()
            .expect("`wrapper_list` is a list")
            .iter()
            .map(|w| wrapper(w).expect("a wrapper list entry is not null"))
            .collect(),
    }
}

/// The puzzle hash of an output's coin: `wrapped_puzzle_hash` for an output
/// that has one, else the bare `puzzle_hash`.
fn coin_puzzle_hash(output: &Value) -> Bytes32 {
    match &output["wrapped_puzzle_hash"] {
        Value::Null => Bytes32::new(bytes(output, "puzzle_hash")),
        _ => Bytes32::new(bytes(output, "wrapped_puzzle_hash")),
    }
}

fn network(value: &Value) -> SilentPaymentNetwork {
    match text(value, "network") {
        "mainnet" => SilentPaymentNetwork::Mainnet,
        "testnet" => SilentPaymentNetwork::Testnet,
        other => panic!("unknown network `{other}`"),
    }
}

// ─── Header ─────────────────────────────────────────────────────────────────

#[test]
fn header() {
    let file = vectors();
    assert_eq!(text(&file, "chip"), "CHIP-0057");
    assert_eq!(
        file["address_version"].as_u64(),
        Some(u64::from(SP_ADDRESS_VERSION))
    );

    // The sections this test walks, so that an empty or reshaped file cannot
    // pass by having nothing to check.
    assert_eq!(list(&file, "payments").len(), 9);
    assert_eq!(list(&file, "settlement_payments").len(), 2);
    assert_eq!(list(&file, "labels").len(), 2);
    assert_eq!(list(&file, "addresses").len(), 4);
    assert_eq!(list(&file, "address_cases").len(), 16);
}

// ─── Payments ───────────────────────────────────────────────────────────────

/// Every payment, as the sender derives it: the key sum, the input hash, the
/// tweak point, and each output's shared secret, tweak, one-time key and
/// puzzle hash.
#[test]
fn payments_as_sent() {
    let file = vectors();

    for payment in list(&file, "payments") {
        payment_as_sent(payment);
    }
}

/// The checks of [`payments_as_sent`] for one entry of `payments` or
/// `settlement_payments`. A settled output is created by a settlement coin, so
/// it may be paid in an asset no coin of the group holds.
fn payment_as_sent(payment: &Value) {
    let name = text(payment, "name");
    let inputs = list(payment, "inputs");
    assert!(!inputs.is_empty(), "{name}: no inputs");

    // Inputs: each synthetic secret key belongs to its public key.
    let sender_sks: Vec<SecretKey> = inputs.iter().map(|i| sk(i, "synthetic_sk")).collect();
    let coin_ids: Vec<Bytes32> = inputs
        .iter()
        .map(|i| Bytes32::new(bytes(i, "coin_id")))
        .collect();
    let mut pk_sum = PublicKey::default();
    for (input, secret) in inputs.iter().zip(&sender_sks) {
        let public = pk(input, "synthetic_pk");
        assert_eq!(secret.public_key(), public, "{name}: synthetic_pk");
        pk_sum += &public;
    }

    // A_sum, from the secret keys and from the public keys.
    let a_sum = aggregate_sender_sks(&sender_sks).unwrap();
    let a_sum_pk = pk(payment, "a_sum_pk");
    assert_eq!(a_sum.public_key(), a_sum_pk, "{name}: a_sum_pk");
    assert_eq!(pk_sum, a_sum_pk, "{name}: a_sum_pk by point addition");

    // coin_id_L, input_hash and the tweak point.
    assert_eq!(
        *coin_ids.iter().min().unwrap(),
        Bytes32::new(bytes(payment, "coin_id_l")),
        "{name}: coin_id_l"
    );
    let input_hash = compute_input_hash(&coin_ids, &a_sum_pk);
    assert_eq!(
        input_hash.to_bytes(),
        bytes::<32>(payment, "input_hash"),
        "{name}: input_hash"
    );
    let tweak_point = compute_tweak_point(&coin_ids, &a_sum_pk)
        .unwrap_or_else(|| panic!("{name}: no tweak point"));
    assert_eq!(
        tweak_point,
        pk(payment, "tweak_point"),
        "{name}: tweak_point"
    );

    let outputs = list(payment, "outputs");
    assert!(!outputs.is_empty(), "{name}: no outputs");
    for output in outputs {
        let k = index(output, "k");
        let at = format!("{name}, output k={k}");
        let recipient = &output["recipient"];

        // Recipient keys, and the spend key of the address that was paid:
        // B_spend, or B_m = B_spend + label_pk for a labeled address.
        let b_scan = sk(recipient, "scan_sk");
        let b_spend = sk(recipient, "spend_sk");
        let scan_pk = pk(recipient, "scan_pk");
        let spend_pk = pk(recipient, "spend_pk");
        assert_eq!(b_scan.public_key(), scan_pk, "{at}: scan_pk");
        assert_eq!(b_spend.public_key(), spend_pk, "{at}: spend_pk");

        let address_spend_pk = pk(recipient, "address_spend_pk");
        let expected_address_spend_pk = match label(recipient) {
            None => spend_pk,
            Some(m) => spend_pk + &generate_label(&b_scan, m).1,
        };
        assert_eq!(
            address_spend_pk, expected_address_spend_pk,
            "{at}: address_spend_pk"
        );

        // The shared secret, from the scanner's side of the ECDH.
        let shared_secret = bytes::<32>(output, "shared_secret");
        assert_eq!(
            compute_shared_secret_from_tweak(&b_scan, &tweak_point),
            shared_secret,
            "{at}: shared_secret"
        );

        // t_k, P_k and the puzzle hash.
        let t_k = derive_output_tweak(&shared_secret, k);
        assert_eq!(t_k.to_bytes(), bytes::<32>(output, "t_k"), "{at}: t_k");
        let onetime_pk = derive_onetime_pk(&address_spend_pk, &t_k);
        assert_eq!(onetime_pk, pk(output, "onetime_pk"), "{at}: onetime_pk");
        let puzzle_hash = Bytes32::new(bytes(output, "puzzle_hash"));
        assert_eq!(
            puzzle_hash_for_pk(&onetime_pk),
            puzzle_hash,
            "{at}: puzzle_hash"
        );

        // The sender's composition arrives at the same puzzle hash.
        assert_eq!(
            derive_one_time_puzzle_hash(&scan_pk, &address_spend_pk, &a_sum, &input_hash, k)
                .unwrap(),
            puzzle_hash,
            "{at}: sender puzzle_hash"
        );

        // spend_tweak = (t_k + label_scalar) mod r, and the one-time key.
        let spend_tweak = match label(recipient) {
            None => t_k,
            Some(m) => t_k.add(&generate_label(&b_scan, m).0),
        };
        assert_eq!(
            spend_tweak.to_bytes(),
            bytes::<32>(output, "spend_tweak"),
            "{at}: spend_tweak"
        );
        let onetime_secret = derive_onetime_sk(&b_spend, &spend_tweak);
        assert_eq!(
            onetime_secret.to_bytes(),
            bytes::<32>(output, "onetime_sk"),
            "{at}: onetime_sk"
        );
        assert_eq!(
            onetime_secret.public_key(),
            onetime_pk,
            "{at}: onetime_sk * G"
        );

        // The coin created: wrap(W, puzzle_hash) for a CAT output; a
        // missing or null wrapper is XCH, whose coin has the bare hash.
        let w = wrapper(&output["wrapper"]);
        if let Some(w) = w {
            assert_eq!(
                w.wrap(puzzle_hash),
                Bytes32::new(bytes(output, "wrapped_puzzle_hash")),
                "{at}: wrapped_puzzle_hash"
            );
        }
        assert_eq!(
            wrap_puzzle_hash(w.as_ref(), puzzle_hash),
            coin_puzzle_hash(output),
            "{at}: coin puzzle hash"
        );

        // The values the Extension to CATs adds to an output.
        if output.get("wrapper").is_some() {
            // The ECDH point, b_scan * T, and shared_secret = sha256 of it.
            let mut ecdh_point = tweak_point;
            ecdh_point.scalar_multiply(&b_scan.to_bytes());
            assert_eq!(ecdh_point, pk(output, "ecdh_point"), "{at}: ecdh_point");
            let mut hasher = Sha256::new();
            hasher.update(ecdh_point.to_bytes());
            assert_eq!(
                hasher.finalize(),
                shared_secret,
                "{at}: shared_secret = sha256(ecdh_point)"
            );

            match w {
                // XCH: no hint, no CAT inner puzzle.
                None => {
                    assert!(output["hint"].is_null(), "{at}: XCH has no hint");
                    assert!(
                        output["cat_inner_puzzle_hash"].is_null(),
                        "{at}: XCH has no CAT inner puzzle hash"
                    );
                }
                // A CAT output is hinted with the bare puzzle hash; the
                // CAT layer's inner puzzle is the standard puzzle, or the
                // revocation layer around it.
                Some(w) => {
                    assert_eq!(
                        Bytes32::new(bytes(output, "hint")),
                        puzzle_hash,
                        "{at}: hint"
                    );
                    assert_eq!(
                        Bytes32::new(bytes(output, "cat_inner_puzzle_hash")),
                        Bytes32::from(
                            CatInfo::new(w.asset_id, w.hidden_puzzle_hash, puzzle_hash)
                                .inner_puzzle_hash()
                        ),
                        "{at}: cat_inner_puzzle_hash"
                    );
                    if w.hidden_puzzle_hash.is_none() {
                        assert_eq!(
                            Bytes32::new(bytes(output, "cat_inner_puzzle_hash")),
                            puzzle_hash,
                            "{at}: a plain CAT's inner puzzle is the standard puzzle"
                        );
                    }
                }
            }
            assert!(
                output["amount"].as_u64().expect("`amount`") > 0,
                "{at}: amount"
            );
        }
    }

    // The block's wrapper list: the wrappers of the payment's coins in the
    // CHIP's order, and every output paid in an asset of the group.
    if payment.get("wrapper_list").is_some() {
        let coin_wrappers: Vec<Option<Wrapper>> =
            inputs.iter().map(|i| wrapper(&i["wrapper"])).collect();
        let output_wrappers: Vec<Option<Wrapper>> =
            outputs.iter().map(|o| wrapper(&o["wrapper"])).collect();
        assert_eq!(
            sort_wrappers(
                coin_wrappers
                    .iter()
                    .chain(&output_wrappers)
                    .copied()
                    .flatten()
            ),
            wrapper_list(payment),
            "{name}: wrapper_list"
        );
        for (output, w) in outputs.iter().zip(&output_wrappers) {
            if output["settled"].as_bool() == Some(true) {
                continue;
            }
            assert!(
                coin_wrappers.contains(w),
                "{name}: an output is paid in an asset of the group"
            );
        }
    }
}

/// Every payment, as a scanner detects it. A `TweakData` is built by hand from
/// the payment's tweak point, the puzzle hashes of all of its output coins
/// (`wrapped_puzzle_hash` for a CAT output) and its wrapper list, and each
/// recipient scans it with its scan secret key and spend public key. It must
/// find exactly its own outputs, with the listed `k`, label, wrapper, bare
/// puzzle hash, `spend_tweak` and one-time secret key.
#[test]
fn payments_as_scanned() {
    let file = vectors();

    for payment in list(&file, "payments") {
        payment_as_scanned(payment);
    }
}

/// The checks of [`payments_as_scanned`] for one entry of `payments` or
/// `settlement_payments`.
fn payment_as_scanned(payment: &Value) {
    let name = text(payment, "name");
    let outputs = list(payment, "outputs");

    // The block as a scanner sees it: one tweak point, every output coin
    // of the payment (identified here by its position), and the block's
    // wrapper list.
    let data = TweakData::with_wrappers(
        vec![pk(payment, "tweak_point")],
        outputs
            .iter()
            .zip(0u8..)
            .map(|(output, position)| OutputMeta {
                puzzle_hash: coin_puzzle_hash(output),
                coin_id: [position; 32].into(),
                amount: 1000 + u64::from(position),
                parent_coin_id: [0xee; 32].into(),
            })
            .collect(),
        wrapper_list(payment),
    );

    // Each distinct recipient scans the payment.
    let recipients: BTreeSet<&str> = outputs
        .iter()
        .map(|output| text(&output["recipient"], "scan_sk"))
        .collect();
    let mut detected_positions = BTreeSet::new();

    for recipient_scan_sk in recipients {
        let mine: Vec<(u8, &Value)> = (0u8..)
            .zip(outputs)
            .filter(|(_, output)| text(&output["recipient"], "scan_sk") == recipient_scan_sk)
            .collect();
        let recipient = &mine[0].1["recipient"];
        let b_scan = sk(recipient, "scan_sk");
        let b_spend = sk(recipient, "spend_sk");
        let spend_pk = pk(recipient, "spend_pk");

        // The recipient's registered labels. The change label m = 0 is
        // always scanned for and is not registered.
        let mut labels = LabelRegistry::new();
        for (_, output) in &mine {
            if let Some(m) = label(&output["recipient"])
                && m != 0
            {
                labels.register(&b_scan, m);
            }
        }

        let detections = scan_from_tweaks(&b_scan, &spend_pk, &data, Some(&labels), K_MAX_DEFAULT);
        assert_eq!(
            detections.len(),
            mine.len(),
            "{name}: recipient {recipient_scan_sk} must find exactly its own outputs"
        );

        for (position, output) in mine {
            let k = index(output, "k");
            let at = format!("{name}, output k={k}");
            let detection = detections
                .iter()
                .find(|d| d.coin_id == Bytes32::from([position; 32]))
                .unwrap_or_else(|| panic!("{at}: not detected"));

            assert_eq!(
                detection.puzzle_hash,
                Bytes32::new(bytes(output, "puzzle_hash")),
                "{at}: puzzle_hash (bare)"
            );
            assert_eq!(
                detection.wrapper,
                wrapper(&output["wrapper"]),
                "{at}: wrapper"
            );
            assert_eq!(
                detection.coin().puzzle_hash,
                coin_puzzle_hash(output),
                "{at}: the coin's puzzle hash"
            );
            assert_eq!(detection.amount, 1000 + u64::from(position), "{at}: coin");
            assert_eq!(detection.k, k, "{at}: k");
            assert_eq!(detection.label, label(&output["recipient"]), "{at}: label");
            assert_eq!(
                detection.tweak.to_bytes(),
                bytes::<32>(output, "spend_tweak"),
                "{at}: spend_tweak"
            );

            // The spend secret key is needed only for this last step.
            let onetime_secret = detection.onetime_sk(&b_spend);
            assert_eq!(
                onetime_secret.to_bytes(),
                bytes::<32>(output, "onetime_sk"),
                "{at}: onetime_sk"
            );
            assert_eq!(
                puzzle_hash_for_pk(&onetime_secret.public_key()),
                detection.puzzle_hash,
                "{at}: the one-time key controls the coin"
            );

            assert!(detected_positions.insert(position), "{at}: detected twice");
        }
    }

    assert_eq!(
        detected_positions.len(),
        outputs.len(),
        "{name}: every output is detected by its recipient"
    );

    negative_controls(payment, &data);
}

/// The number of detections every recipient of `payment` finds in `data`,
/// with its registered labels.
fn detections_by_recipients(payment: &Value, data: &TweakData) -> usize {
    let outputs = list(payment, "outputs");
    let recipients: BTreeSet<&str> = outputs
        .iter()
        .map(|output| text(&output["recipient"], "scan_sk"))
        .collect();
    recipients
        .into_iter()
        .map(|recipient_scan_sk| {
            let mine: Vec<&Value> = outputs
                .iter()
                .filter(|o| text(&o["recipient"], "scan_sk") == recipient_scan_sk)
                .collect();
            let b_scan = sk(&mine[0]["recipient"], "scan_sk");
            let mut labels = LabelRegistry::new();
            for output in &mine {
                if let Some(m) = label(&output["recipient"])
                    && m != 0
                {
                    labels.register(&b_scan, m);
                }
            }
            let spend_pk = pk(&mine[0]["recipient"], "spend_pk");
            scan_from_tweaks(&b_scan, &spend_pk, data, Some(&labels), K_MAX_DEFAULT).len()
        })
        .sum()
}

fn count(controls: &Value, key: &str) -> usize {
    usize::try_from(
        controls[key]
            .as_u64()
            .unwrap_or_else(|| panic!("`{key}` is a count")),
    )
    .unwrap()
}

/// The negative controls of a payment of the Extension to CATs, each read
/// only where the entry has it. `data` is the block as the scanner sees it
/// (the payment's tweak point, its output coins and its wrapper list).
fn negative_controls(payment: &Value, data: &TweakData) {
    let controls = &payment["negative_controls"];
    if controls.is_null() {
        return;
    }
    let name = text(payment, "name");
    let outputs = list(payment, "outputs");
    let coin_hashes: Vec<Bytes32> = outputs.iter().map(coin_puzzle_hash).collect();

    // Without the block's wrapper list only bare candidates are tested.
    if !controls["detections_without_wrapper_list"].is_null() {
        let bare = TweakData::new(data.tweak_points.clone(), data.outputs.clone());
        assert_eq!(
            detections_by_recipients(payment, &bare),
            count(controls, "detections_without_wrapper_list"),
            "{name}: detections_without_wrapper_list"
        );
    }

    // Vector 10: a scanner without the Extension to CATs takes the XCH coin
    // alone as its group.
    if !controls["core_only_tweak_point"].is_null() {
        let input = &list(payment, "inputs")[0];
        assert!(
            input["wrapper"].is_null(),
            "{name}: input 0 is the XCH coin"
        );
        let a_0 = pk(input, "synthetic_pk");
        let core = compute_tweak_point(&[Bytes32::new(bytes(input, "coin_id"))], &a_0)
            .expect("a tweak point");
        assert_eq!(
            core,
            pk(controls, "core_only_tweak_point"),
            "{name}: core_only_tweak_point"
        );
        assert_ne!(core, pk(payment, "tweak_point"));

        let recipient = &outputs[0]["recipient"];
        let t_0 = derive_output_tweak(
            &compute_shared_secret_from_tweak(&sk(recipient, "scan_sk"), &core),
            0,
        );
        let candidate = puzzle_hash_for_pk(&derive_onetime_pk(&pk(recipient, "spend_pk"), &t_0));
        assert_eq!(
            candidate,
            Bytes32::new(bytes(controls, "core_only_puzzle_hash")),
            "{name}: core_only_puzzle_hash"
        );
        assert!(!coin_hashes.contains(&candidate));
        let core_only = TweakData::new(vec![core], data.outputs.clone());
        assert_eq!(
            detections_by_recipients(payment, &core_only),
            count(controls, "detections_by_core_only_scanner"),
            "{name}: detections_by_core_only_scanner"
        );
    }

    // Vector 11: the same output without the revocation layer is another
    // coin, and a scan under the plain-CAT wrapper finds nothing.
    if !controls["plain_cat_wrapped_puzzle_hash"].is_null() {
        let output = &outputs[0];
        let w = wrapper(&output["wrapper"]).expect("a CAT output");
        assert!(w.hidden_puzzle_hash.is_some(), "{name}: a revocable CAT");
        let plain = Wrapper::cat(w.asset_id);
        let p = Bytes32::new(bytes(output, "puzzle_hash"));
        assert_eq!(
            plain.wrap(p),
            Bytes32::new(bytes(controls, "plain_cat_wrapped_puzzle_hash")),
            "{name}: plain_cat_wrapped_puzzle_hash"
        );
        assert!(!coin_hashes.contains(&plain.wrap(p)));
        let under_plain =
            TweakData::with_wrappers(data.tweak_points.clone(), data.outputs.clone(), vec![plain]);
        assert_eq!(
            detections_by_recipients(payment, &under_plain),
            count(controls, "detections_with_plain_cat_wrapper"),
            "{name}: detections_with_plain_cat_wrapper"
        );
    }

    // Vector 11: the input coin spent through the issuer's hidden path is not
    // an eligible spend; on the holder's path it is (A, W). The spends are
    // built by the SDK; the vector's coin IDs do not matter for eligibility.
    if let Some(expected) = controls["hidden_path_is_eligible"].as_bool() {
        let input = &list(payment, "inputs")[0];
        let key = pk(input, "synthetic_pk");
        let w = wrapper(&input["wrapper"]).expect("a CAT input");
        let holder = revocable_input_spend(input, w, false);
        let issuer = revocable_input_spend(input, w, true);
        assert_eq!(
            eligible_spend(&holder),
            Some((key, Some(w))),
            "{name}: holder path"
        );
        assert_eq!(
            eligible_spend(&issuer).is_some(),
            expected,
            "{name}: hidden_path_is_eligible"
        );
    }
}

/// A spend of a payment's revocable CAT input, held in the standard puzzle of
/// its synthetic key, on the holder's path or (`revoke`) the issuer's hidden
/// path, built with the SDK's `Cat::spend_all`.
fn revocable_input_spend(input: &Value, w: Wrapper, revoke: bool) -> chia_protocol::CoinSpend {
    let key = pk(input, "synthetic_pk");
    let info = CatInfo::new(
        w.asset_id,
        w.hidden_puzzle_hash,
        StandardArgs::curry_tree_hash(key).into(),
    );
    let parent = Bytes32::new(bytes(input, "coin_id"));
    let cat = Cat::new(
        chia_protocol::Coin::new(parent, info.puzzle_hash().into(), 1000),
        Some(LineageProof {
            parent_parent_coin_info: parent,
            parent_inner_puzzle_hash: info.inner_puzzle_hash().into(),
            parent_amount: 1000,
        }),
        info,
    );
    let mut ctx = SpendContext::new();
    let inner = StandardLayer::new(key)
        .spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(Bytes32::new([0x77; 32]), 1000, Memos::None),
        )
        .expect("inner spend");
    let spend = if revoke {
        CatSpend::revoke(cat, inner)
    } else {
        CatSpend::new(cat, inner)
    };
    Cat::spend_all(&mut ctx, &[spend]).expect("spend_all");
    ctx.take().pop().expect("one coin spend")
}

/// A payment of `payments`, by name.
fn payment<'a>(file: &'a Value, name: &str) -> &'a Value {
    list(file, "payments")
        .iter()
        .find(|p| text(p, "name") == name)
        .unwrap_or_else(|| panic!("no payment `{name}`"))
}

/// Vector 9, the tracer of the Extension to CATs: one CAT output found at
/// `k = 0` under the block's wrapper list `[W]`, with the bare one-time puzzle
/// hash, the wrapper, the wrapped coin and the one-time key of the file; and
/// missed when the wrapper list is empty.
#[test]
fn vector_9_cat_output_found_only_under_the_wrapper_list() {
    let file = vectors();
    let v9 = payment(&file, "vector_9_cat_payment");
    let output = &list(v9, "outputs")[0];
    let recipient = &output["recipient"];

    let w = Wrapper::cat(Bytes32::new(bytes(&v9["inputs"][0]["wrapper"], "asset_id")));
    assert_eq!(wrapper_list(v9), vec![w]);
    assert_eq!(
        w.asset_id,
        Bytes32::new(hex_literal::hex!(
            "a031232e72df4aa270c253a530b9b0d782d46ac7b5dad63c2aa37690bbb37f6d"
        ))
    );

    let coin_ph = Bytes32::new(bytes(output, "wrapped_puzzle_hash"));
    let bare = Bytes32::new(bytes(output, "puzzle_hash"));
    let meta = OutputMeta {
        puzzle_hash: coin_ph,
        coin_id: [9u8; 32].into(),
        amount: 1000,
        parent_coin_id: [0xee; 32].into(),
    };
    let b_scan = sk(recipient, "scan_sk");
    let b_spend = sk(recipient, "spend_sk");
    let spend_pk = pk(recipient, "spend_pk");

    let data = TweakData::with_wrappers(vec![pk(v9, "tweak_point")], vec![meta], vec![w]);
    let detections = scan_from_tweaks(&b_scan, &spend_pk, &data, None, K_MAX_DEFAULT);
    assert_eq!(detections.len(), 1);
    let d = &detections[0];
    assert_eq!(d.k, 0);
    assert_eq!(d.label, None);
    assert_eq!(d.wrapper, Some(w));
    assert_eq!(
        d.puzzle_hash,
        Bytes32::new(hex_literal::hex!(
            "df83709ebd837a46717cfb59cc3c75de70606ffb5e3499c202339cf9a5a20dd4"
        ))
    );
    assert_eq!(d.puzzle_hash, bare);
    assert_eq!(
        d.coin().puzzle_hash,
        Bytes32::new(hex_literal::hex!(
            "cf9f1b5dbf2bd67fa0bb69a554e38c0f2d53877fe565f34f16a16df9f6fa18e5"
        ))
    );
    assert_eq!(d.coin_id, Bytes32::from([9u8; 32]));
    assert_eq!(d.tweak.to_bytes(), bytes::<32>(output, "spend_tweak"));
    assert_eq!(
        d.onetime_sk(&b_spend).to_bytes(),
        bytes::<32>(output, "onetime_sk")
    );

    // Without the wrapper list only the bare candidate is tested: missed.
    let bare_only = TweakData::new(data.tweak_points.clone(), data.outputs.clone());
    assert!(scan_from_tweaks(&b_scan, &spend_pk, &bare_only, None, K_MAX_DEFAULT).is_empty());
}

// ─── Settlement payments ────────────────────────────────────────────────────

/// `value` serialized as CLVM.
fn serialized<T: ToClvm<Allocator>>(ctx: &mut SpendContext, value: &T) -> Vec<u8> {
    let ptr = ctx.alloc(value).expect("alloc");
    node_to_bytes(ctx, ptr).expect("serialize")
}

/// A payment of the file, given as serialized CLVM `(puzzle_hash amount . memos)`.
fn decoded_payment(ctx: &mut SpendContext, value: &Value, key: &str) -> Payment {
    let bytes = hex::decode(text(value, key)).expect("`payment` is hex");
    let ptr = node_from_bytes(ctx, &bytes).expect("`payment` is CLVM");
    ctx.extract::<Payment>(ptr).expect("`payment` is a payment")
}

fn amount(value: &Value) -> u64 {
    value["amount"].as_u64().expect("`amount` is an integer")
}

fn settled(output: &Value) -> bool {
    output["settled"].as_bool().expect("`settled` is a boolean")
}

/// `DeriveSettlementOutputs` over a settlement entry: its inputs as the spend
/// group, its outputs as the recipient entries (the spend key of the address
/// that is paid, the wrapper, the settled flag and the amount), `other_payments`
/// decoded from their serialized payments, and its nonce.
fn derive_entry(
    ctx: &mut SpendContext,
    entry: &Value,
    other_payments: &Value,
) -> SettlementDerivation {
    let name = text(entry, "name");
    // An input of the file is a coin ID, a synthetic key pair and a wrapper;
    // the coin is the standard puzzle curried with `synthetic_pk`, wrapped in
    // the input's wrapper, so its puzzle hash is
    // `wrap(W, curry_tree_hash(synthetic_pk))`. The file gives no other coin
    // record (its coin IDs are SHA256 of a name), so the entry is built from
    // these fields, not with `SettlementGroupCoin::new`.
    let group: Vec<SettlementGroupCoin> = list(entry, "inputs")
        .iter()
        .map(|i| {
            let secret_key = sk(i, "synthetic_sk");
            let public_key = pk(i, "synthetic_pk");
            assert_eq!(secret_key.public_key(), public_key, "{name}: input key");
            let w = wrapper(&i["wrapper"]);
            SettlementGroupCoin {
                coin_id: Bytes32::new(bytes(i, "coin_id")),
                puzzle_hash: wrap_puzzle_hash(
                    w.as_ref(),
                    StandardArgs::curry_tree_hash(public_key).into(),
                ),
                secret_key: SyntheticSecretKey::from_synthetic_unchecked(secret_key),
                wrapper: w,
            }
        })
        .collect();
    let recipients: Vec<SettlementRecipient> = list(entry, "outputs")
        .iter()
        .map(|output| SettlementRecipient {
            scan_pk: pk(&output["recipient"], "scan_pk"),
            spend_pk: pk(&output["recipient"], "address_spend_pk"),
            wrapper: wrapper(&output["wrapper"]),
            settled: settled(output),
            amount: amount(output),
        })
        .collect();
    let others: Vec<OtherPayment> = other_payments
        .as_array()
        .expect("`other_payments` is a list")
        .iter()
        .map(|other| {
            let w = wrapper(&other["wrapper"]);
            let payment = decoded_payment(ctx, other, "payment");
            // An other payment is listed as the party lists it in its offer:
            // here (q amount ()) for XCH, the CHIP's encoding.
            assert_eq!(
                payment,
                encoded_payment(
                    ctx,
                    Bytes32::new(bytes(other, "puzzle_hash")),
                    amount(other),
                    w.as_ref()
                )
                .unwrap(),
                "{name}: other payment"
            );
            OtherPayment {
                wrapper: w,
                payment,
            }
        })
        .collect();
    derive_settlement_outputs(
        ctx,
        &group,
        &recipients,
        &others,
        Bytes32::new(bytes(entry, "nonce")),
    )
    .unwrap_or_else(|e| panic!("{name}: DeriveSettlementOutputs failed: {e}"))
}

fn hashes(value: &Value, key: &str) -> Vec<Bytes32> {
    list(value, key)
        .iter()
        .map(|h| {
            let decoded = hex::decode(h.as_str().expect("a hex string")).expect("hex");
            Bytes32::new(decoded.try_into().expect("32 bytes"))
        })
        .collect()
}

/// Every entry of `settlement_payments` (Vectors 12 and 13), as the party
/// derives it: the spend group and each output as for `payments`, then
/// `DeriveSettlementOutputs`, which must give the entry's `settlement_outputs`,
/// each settled output's encoded `payment`, the `notarized_payments` (wrapper,
/// settlement puzzle hash, payments, the serialized notarized payment, its
/// tree hash and the announcement), the `assertions` and the `group_outputs`;
/// as a negative control, the announcement of the settled payment listed
/// without its memo list; and, for Vector 13, `two_payments_in_one_asset`
/// with its `per_payment_announcements`.
#[test]
fn settlement_payments_as_sent() {
    let file = vectors();
    let entries = list(&file, "settlement_payments");
    assert_eq!(entries.len(), 2);
    let mut two_payments_checked = 0;

    for entry in entries {
        let name = text(entry, "name");
        payment_as_sent(entry);

        let mut ctx = SpendContext::new();
        let derivation = derive_entry(&mut ctx, entry, &entry["other_payments"]);
        let outputs = list(entry, "outputs");
        let nonce = Bytes32::new(bytes(entry, "nonce"));

        // settlement_outputs: wrap(W, p) of each settled output.
        assert_eq!(
            derivation.settlement_outputs,
            hashes(entry, "settlement_outputs"),
            "{name}: settlement_outputs"
        );
        let settled_outputs: Vec<&Value> = outputs.iter().filter(|o| settled(o)).collect();
        assert_eq!(derivation.settled_payments.len(), settled_outputs.len());
        for (output, payment) in settled_outputs.iter().zip(&derivation.settled_payments) {
            let at = format!("{name}, output k={}", index(output, "k"));
            assert_eq!(
                hex::encode(serialized(&mut ctx, payment)),
                text(output, "payment"),
                "{at}: payment"
            );
            assert!(
                derivation
                    .settlement_outputs
                    .contains(&coin_puzzle_hash(output)),
                "{at}: the settlement coin creates the output coin"
            );
        }
        for output in outputs.iter().filter(|o| !settled(o)) {
            assert!(output["payment"].is_null(), "{name}: unsettled payment");
        }

        // One notarized payment, and one assertion, per asset.
        let expected = list(entry, "notarized_payments");
        assert_eq!(
            derivation.notarized_payments.len(),
            expected.len(),
            "{name}: notarized_payments"
        );
        for (np, expected) in derivation.notarized_payments.iter().zip(expected) {
            assert_eq!(np.wrapper, wrapper(&expected["wrapper"]), "{name}: wrapper");
            assert_eq!(
                np.settlement_puzzle_hash,
                Bytes32::new(bytes(expected, "settlement_puzzle_hash")),
                "{name}: settlement_puzzle_hash"
            );
            assert_eq!(
                np.settlement_puzzle_hash,
                settlement_puzzle_hash(np.wrapper.as_ref())
            );
            let payments: Vec<String> = np
                .notarized_payment
                .payments
                .iter()
                .map(|payment| hex::encode(serialized(&mut ctx, payment)))
                .collect();
            let expected_payments: Vec<&str> = list(expected, "payments")
                .iter()
                .map(|p| p.as_str().expect("a hex string"))
                .collect();
            assert_eq!(payments, expected_payments, "{name}: payments");
            assert_eq!(
                np.notarized_payment.nonce, nonce,
                "{name}: the nonce of the notarized payment"
            );
            let ptr = ctx.alloc(&np.notarized_payment).unwrap();
            assert_eq!(
                hex::encode(node_to_bytes(&ctx, ptr).unwrap()),
                text(expected, "notarized_payment"),
                "{name}: notarized_payment"
            );
            assert_eq!(
                Bytes32::from(ctx.tree_hash(ptr)),
                Bytes32::new(bytes(expected, "notarized_tree_hash")),
                "{name}: notarized_tree_hash"
            );
            assert_eq!(
                np.announcement,
                Bytes32::new(bytes(expected, "announcement")),
                "{name}: announcement"
            );
        }
        assert_eq!(
            derivation
                .assertions
                .iter()
                .map(|a| a.announcement_id)
                .collect::<Vec<_>>(),
            hashes(entry, "assertions"),
            "{name}: assertions"
        );

        // group_outputs: the coins of the outputs the group creates, at their k.
        assert_eq!(
            derivation
                .group_outputs
                .iter()
                .map(|g| g.wrapped_puzzle_hash)
                .collect::<Vec<_>>(),
            hashes(entry, "group_outputs"),
            "{name}: group_outputs"
        );
        for (group_output, output) in derivation
            .group_outputs
            .iter()
            .zip(outputs.iter().filter(|o| !settled(o)))
        {
            assert_eq!(group_output.k, index(output, "k"), "{name}: group output k");
            assert_eq!(
                group_output.puzzle_hash,
                Bytes32::new(bytes(output, "puzzle_hash")),
                "{name}: group output puzzle_hash"
            );
            assert_eq!(group_output.wrapper, wrapper(&output["wrapper"]));
        }

        // Negative control: the settled payment listed without its memo list,
        // (p amount), gives another announcement.
        let controls = &entry["negative_controls"];
        assert!(
            !controls["no_memo_announcement"].is_null(),
            "{name}: no_memo_announcement"
        );
        assert_eq!(settled_outputs.len(), 1, "{name}: one settled output");
        let output = settled_outputs[0];
        let w = wrapper(&output["wrapper"]);
        let without_memos = Payment::new(
            Bytes32::new(bytes(output, "puzzle_hash")),
            amount(output),
            Memos::None,
        );
        let no_memo = settlement_announcement(
            &ctx,
            w.as_ref(),
            &NotarizedPayment::new(nonce, vec![without_memos]),
        );
        assert_eq!(
            no_memo,
            Bytes32::new(bytes(controls, "no_memo_announcement")),
            "{name}: no_memo_announcement"
        );
        assert!(
            !derivation
                .notarized_payments
                .iter()
                .any(|np| np.announcement == no_memo)
        );

        // Vector 13: a second requested payment in the same asset, at an
        // address that is not a silent payment address. The silent payment is
        // listed first, and one announcement covers both; neither payment's
        // own announcement is that one.
        if let Some(two) = entry.get("two_payments_in_one_asset") {
            two_payments_checked += 1;
            let derivation = derive_entry(&mut ctx, entry, &two["other_payments"]);
            assert_eq!(derivation.notarized_payments.len(), 1, "{name}: one asset");
            let np = &derivation.notarized_payments[0];
            let payments: Vec<String> = np
                .notarized_payment
                .payments
                .iter()
                .map(|payment| hex::encode(serialized(&mut ctx, payment)))
                .collect();
            let expected_payments: Vec<&str> = list(two, "payments")
                .iter()
                .map(|p| p.as_str().expect("a hex string"))
                .collect();
            assert_eq!(payments, expected_payments, "{name}: two payments");
            let ptr = ctx.alloc(&np.notarized_payment).unwrap();
            assert_eq!(
                hex::encode(node_to_bytes(&ctx, ptr).unwrap()),
                text(two, "notarized_payment"),
                "{name}: two payments, notarized_payment"
            );
            assert_eq!(
                Bytes32::from(ctx.tree_hash(ptr)),
                Bytes32::new(bytes(two, "notarized_tree_hash")),
                "{name}: two payments, notarized_tree_hash"
            );
            let announcement = Bytes32::new(bytes(two, "announcement"));
            assert_eq!(np.announcement, announcement, "{name}: two payments");
            assert_eq!(
                derivation
                    .assertions
                    .iter()
                    .map(|a| a.announcement_id)
                    .collect::<Vec<_>>(),
                vec![announcement],
                "{name}: one assertion over both payments"
            );
            let per_payment = hashes(two, "per_payment_announcements");
            assert_eq!(per_payment.len(), np.notarized_payment.payments.len());
            for (payment, expected) in np.notarized_payment.payments.iter().zip(&per_payment) {
                let alone = settlement_announcement(
                    &ctx,
                    np.wrapper.as_ref(),
                    &NotarizedPayment::new(nonce, vec![payment.clone()]),
                );
                assert_eq!(alone, *expected, "{name}: per_payment_announcements");
                assert_ne!(alone, announcement);
            }
        }
    }
    assert_eq!(
        two_payments_checked, 1,
        "Vector 13 lists two payments in one asset"
    );
}

/// Every entry of `settlement_payments`, as the party's scanner detects it: a
/// `TweakData` with the entry's tweak point, every output coin (the settled
/// ones created by the settlement coin, under the wrapper of their asset) and
/// the block's wrapper list; the party finds each output with its `k`, label,
/// wrapper and bare puzzle hash, and without the wrapper list it finds
/// `detections_without_wrapper_list` of them.
#[test]
fn settlement_payments_as_scanned() {
    let file = vectors();
    for entry in list(&file, "settlement_payments") {
        assert!(
            !entry["negative_controls"]["detections_without_wrapper_list"].is_null(),
            "{}: detections_without_wrapper_list",
            text(entry, "name")
        );
        payment_as_scanned(entry);
    }
}

// ─── Labels ─────────────────────────────────────────────────────────────────

#[test]
fn labels() {
    let file = vectors();

    for entry in list(&file, "labels") {
        let m = index(entry, "m");
        let (label_scalar, label_pk) = generate_label(&sk(entry, "scan_sk"), m);
        assert_eq!(
            label_scalar.to_bytes(),
            bytes::<32>(entry, "label_scalar"),
            "label {m}: label_scalar"
        );
        assert_eq!(label_pk, pk(entry, "label_pk"), "label {m}: label_pk");

        // label_pk = label_scalar * G.
        assert_eq!(
            SecretKey::from_bytes(&label_scalar.to_bytes())
                .unwrap()
                .public_key(),
            label_pk
        );
    }
}

// ─── Addresses ──────────────────────────────────────────────────────────────

/// Every address round-trips: the keys encode to the listed string, and the
/// string decodes to the keys and the network.
#[test]
fn addresses() {
    let file = vectors();

    for entry in list(&file, "addresses") {
        let encoded = text(entry, "address");
        let address =
            SilentPaymentAddress::new(pk(entry, "scan_pk"), pk(entry, "spend_pk"), network(entry));

        assert_eq!(address.encode().unwrap(), encoded);
        assert_eq!(SilentPaymentAddress::decode(encoded).unwrap(), address);
    }
}

/// Every address case: a valid one decodes to the listed keys, an invalid one
/// is rejected.
///
/// All cases are evaluated before anything is asserted, so that a failure
/// lists every case the decoder judges differently from the CHIP.
#[test]
fn address_cases() {
    let file = vectors();
    let cases = list(&file, "address_cases");

    let mut valid = 0;
    let mut wrong = Vec::new();

    for case in cases {
        let description = text(case, "description");
        let decoded = SilentPaymentAddress::decode(text(case, "address"));

        if case["valid"].as_bool().expect("`valid` is a boolean") {
            valid += 1;
            match decoded {
                Ok(address) => {
                    if address.scan_pk != pk(case, "scan_pk")
                        || address.spend_pk != pk(case, "spend_pk")
                    {
                        wrong.push(format!("{description}: decoded to other keys"));
                    }
                }
                Err(error) => wrong.push(format!("{description}: rejected with `{error}`")),
            }
        } else if decoded.is_ok() {
            wrong.push(format!("{description}: accepted"));
        }
    }

    assert_eq!(valid, 3, "the file has three valid address cases");
    assert!(
        wrong.is_empty(),
        "address cases judged differently from the CHIP:\n{}",
        wrong.join("\n")
    );
}

// ─── Key derivation ─────────────────────────────────────────────────────────

/// A derivation path in the notation of the file, with `n` marking a hardened
/// step: `m/12381n/8444n/12n/0n`.
fn hardened_path(path: &[u32]) -> String {
    path.iter().fold("m".to_string(), |mut out, step| {
        out.push_str(&format!("/{step}n"));
        out
    })
}

#[test]
fn key_derivation() {
    let file = vectors();
    let entry = &file["key_derivation"];

    // The SDK derives with an empty BIP-39 passphrase, hardened at every level
    // of these paths.
    assert_eq!(text(entry, "passphrase"), "");
    assert_eq!(text(entry, "scan_path"), hardened_path(SCAN_PATH));
    assert_eq!(text(entry, "spend_path"), hardened_path(SPEND_PATH));

    let mnemonic = Mnemonic::parse(text(entry, "mnemonic")).expect("a BIP-39 mnemonic");
    let master = SecretKey::from_seed(&mnemonic.to_seed(text(entry, "passphrase")));
    assert_eq!(master.to_bytes(), bytes::<32>(entry, "master_sk"));
    assert_eq!(master.public_key(), pk(entry, "master_pk"));

    let keys = SilentPaymentKeys::from_mnemonic(&mnemonic);
    assert_eq!(keys.scan_sk().to_bytes(), bytes::<32>(entry, "scan_sk"));
    assert_eq!(*keys.scan_pk(), pk(entry, "scan_pk"));
    assert_eq!(keys.spend_sk().to_bytes(), bytes::<32>(entry, "spend_sk"));
    assert_eq!(*keys.spend_pk(), pk(entry, "spend_pk"));

    assert_eq!(
        keys.unlabeled_address(SilentPaymentNetwork::Mainnet)
            .encode()
            .unwrap(),
        text(entry, "mainnet_address")
    );
    assert_eq!(
        keys.unlabeled_address(SilentPaymentNetwork::Testnet)
            .encode()
            .unwrap(),
        text(entry, "testnet_address")
    );

    // The derived scalars are not the given recipient keys of the payments.
    let derived = ScalarField::from_bytes_raw(keys.scan_sk().to_bytes());
    for payment in list(&file, "payments") {
        for output in list(payment, "outputs") {
            assert_ne!(
                derived.to_bytes(),
                bytes::<32>(&output["recipient"], "scan_sk")
            );
        }
    }
}
