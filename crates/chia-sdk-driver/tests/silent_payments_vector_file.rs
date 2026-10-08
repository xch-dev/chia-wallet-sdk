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

use std::collections::BTreeSet;

use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_sdk_driver::silent_payments::{
    K_MAX_DEFAULT, OutputMeta, TweakData, Wrapper, aggregate_sender_sks, compute_input_hash,
    compute_shared_secret_from_tweak, compute_tweak_point, derive_one_time_puzzle_hash,
    derive_onetime_pk, derive_onetime_sk, derive_output_tweak, puzzle_hash_for_pk,
    scan_from_tweaks, wrap_puzzle_hash,
};
use chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH, ScalarField};
use chia_sdk_utils::silent_payments::{
    LabelRegistry, SP_ADDRESS_VERSION, SilentPaymentAddress, SilentPaymentKeys,
    SilentPaymentNetwork, generate_label,
};
use chia_sha2::Sha256;
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

            let detections =
                scan_from_tweaks(&b_scan, &spend_pk, &data, Some(&labels), K_MAX_DEFAULT);
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
    }
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
