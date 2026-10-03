#![cfg(feature = "chip-0057")]

//! The test vectors of CHIP-0057 ("Test Cases"), with every intermediate value
//! the CHIP lists pinned to the hex in its text.
//!
//! Vectors 1 through 7 treat the recipient's scan and spend secret keys as
//! given values. The sender's keys are derived from the stated mnemonic at
//! Chia's standard wallet path, `m/12381/8444/2/<index>` (unhardened), and
//! then made synthetic. Vector 8 (key derivation) is pinned next to
//! `SilentPaymentKeys::from_mnemonic` in `chia-sdk-utils`.

use bip39::Mnemonic;
use chia_bls::{DerivableKey, PublicKey, SecretKey};
use chia_protocol::Bytes32;
use chia_puzzle_types::DeriveSynthetic;
use chia_sdk_driver::silent_payments::{
    DetectedSpCoin, K_MAX_DEFAULT, OutputMeta, TweakData, aggregate_sender_sks, compute_input_hash,
    compute_shared_secret_from_tweak, derive_one_time_puzzle_hash, derive_onetime_pk,
    derive_onetime_sk, derive_output_tweak, puzzle_hash_for_pk, scan_from_tweaks,
};
use chia_sdk_types::silent_payments::ScalarField;
use chia_sdk_utils::Address;
use chia_sdk_utils::silent_payments::{
    LabelRegistry, SilentPaymentAddress, SilentPaymentKeys, SilentPaymentNetwork, generate_label,
};
use chia_sha2::Sha256;
use hex_literal::hex;

const SENDER_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

// ─── Given recipient keys (vectors 1 through 7) ────────────────────────────

const B_SCAN: [u8; 32] = hex!("132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6");
const B_SCAN_PK: [u8; 48] = hex!(
    "a04f404bfbfdc9311736899fe32d2275bb007814510c3523529487ad75736075"
    "73ade20d31c75107b40331fff79ac896"
);
const B_SPEND: [u8; 32] = hex!("53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087");
const B_SPEND_PK: [u8; 48] = hex!(
    "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
    "f6f31e22e0831ba5088c952625a75c74"
);

// ─── Sender keys ────────────────────────────────────────────────────────────

const SENDER_WALLET_SK_0: [u8; 32] =
    hex!("6c8d1a9f97413f8d8e8c158f5bc875b58b498de05c9109b4dc240280d32e2a31");
const A_SYN_0: [u8; 32] = hex!("5002eaf015c1c3a9694cc054e96273279732f4f963616ff89b6d4addcd678c7a");
const A_0: [u8; 48] = hex!(
    "8d9a5ed9c9b1a58476b07262007c636d775f2a33f0533737f3b3b0eaf99a8c0c"
    "51b3f2d87dc03a657e07f1828ab760fa"
);
const A_SYN_1: [u8; 32] = hex!("05fded8808216b65d439fc41cb07c7270e37ed743e0745652afe055cfe91cf0f");
const A_1: [u8; 48] = hex!(
    "94c5c19f4343bc2655af729469285a392de9048851363b0b1329a4539a46ab4c"
    "6e8bfb39d32da25bffe4d9cdbe3e1061"
);

// ─── Helpers ────────────────────────────────────────────────────────────────

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize()
}

fn sk(bytes: [u8; 32]) -> SecretKey {
    SecretKey::from_bytes(&bytes).expect("secret key below the group order")
}

fn pk(bytes: [u8; 48]) -> PublicKey {
    PublicKey::from_bytes(&bytes).expect("valid G1 element")
}

/// The sender's synthetic secret key at `m/12381/8444/2/<index>`.
fn sender_synthetic_sk(index: u32) -> SecretKey {
    let seed = Mnemonic::parse(SENDER_MNEMONIC).unwrap().to_seed("");
    let mut key = SecretKey::from_seed(&seed);
    for step in [12381, 8444, 2, index] {
        key = key.derive_unhardened(step);
    }
    key.derive_synthetic()
}

fn recipient() -> SilentPaymentKeys {
    SilentPaymentKeys::from_secret_keys(sk(B_SCAN), sk(B_SPEND))
}

fn txch(puzzle_hash: Bytes32) -> String {
    Address::new(puzzle_hash, "txch".to_string())
        .encode()
        .unwrap()
}

/// Every intermediate value of the `SendSilentPayment` procedure for one output.
struct Sent {
    input_hash: [u8; 32],
    /// The ECDH point `S = (input_hash * a_sum) * B_scan`.
    ecdh_point: [u8; 48],
    shared_secret: [u8; 32],
    tweak: [u8; 32],
    onetime_pk: [u8; 48],
    puzzle_hash: Bytes32,
}

/// Runs the sender's derivation step by step, and checks that the SDK's
/// `derive_one_time_puzzle_hash` arrives at the same puzzle hash.
fn send(
    sender_sks: &[SecretKey],
    coin_ids: &[Bytes32],
    scan_pk: &PublicKey,
    spend_pk: &PublicKey,
    k: u32,
) -> Sent {
    let a_sum = aggregate_sender_sks(sender_sks).unwrap();
    let a_sum_pk = sk(a_sum.to_bytes()).public_key();
    let input_hash = compute_input_hash(coin_ids, &a_sum_pk);

    let mut ecdh_point = *scan_pk;
    ecdh_point.scalar_multiply(&a_sum.to_bytes());
    ecdh_point.scalar_multiply(&input_hash.to_bytes());

    let shared_secret = sha256(&ecdh_point.to_bytes());
    let tweak = derive_output_tweak(&shared_secret, k);
    let onetime_pk = derive_onetime_pk(spend_pk, &tweak);
    let puzzle_hash = puzzle_hash_for_pk(&onetime_pk);

    assert_eq!(
        derive_one_time_puzzle_hash(scan_pk, spend_pk, &a_sum, &input_hash, k).unwrap(),
        puzzle_hash
    );

    Sent {
        input_hash: input_hash.to_bytes(),
        ecdh_point: ecdh_point.to_bytes(),
        shared_secret,
        tweak: tweak.to_bytes(),
        onetime_pk: onetime_pk.to_bytes(),
        puzzle_hash,
    }
}

/// The tweak point `T = input_hash * A_sum` a scanner receives for a group.
fn tweak_point(a_sum: &PublicKey, input_hash: [u8; 32]) -> PublicKey {
    let mut point = *a_sum;
    point.scalar_multiply(&input_hash);
    point
}

fn output(puzzle_hash: Bytes32, id: u8) -> OutputMeta {
    OutputMeta {
        puzzle_hash,
        coin_id: [id; 32].into(),
        amount: 1000,
        parent_coin_id: [0u8; 32].into(),
    }
}

fn scan(
    keys: &SilentPaymentKeys,
    tweak_points: Vec<PublicKey>,
    outputs: Vec<OutputMeta>,
    labels: Option<&LabelRegistry>,
) -> Vec<DetectedSpCoin> {
    scan_from_tweaks(
        keys.scan_sk(),
        keys.spend_pk(),
        &TweakData {
            tweak_points,
            outputs,
        },
        labels,
        K_MAX_DEFAULT,
    )
}

// ─── Key derivation tables ──────────────────────────────────────────────────

#[test]
fn sender_and_recipient_keys() {
    let seed = Mnemonic::parse(SENDER_MNEMONIC).unwrap().to_seed("");
    let mut wallet_sk = SecretKey::from_seed(&seed);
    for step in [12381, 8444, 2, 0] {
        wallet_sk = wallet_sk.derive_unhardened(step);
    }
    assert_eq!(wallet_sk.to_bytes(), SENDER_WALLET_SK_0);

    assert_eq!(sender_synthetic_sk(0).to_bytes(), A_SYN_0);
    assert_eq!(sender_synthetic_sk(0).public_key().to_bytes(), A_0);
    assert_eq!(sender_synthetic_sk(1).to_bytes(), A_SYN_1);
    assert_eq!(sender_synthetic_sk(1).public_key().to_bytes(), A_1);

    let keys = recipient();
    assert_eq!(keys.scan_pk().to_bytes(), B_SCAN_PK);
    assert_eq!(keys.spend_pk().to_bytes(), B_SPEND_PK);
}

// ─── Test Vector 1: Single Output Payment ───────────────────────────────────

const TV1_COIN_ID: [u8; 32] =
    hex!("5d759d2d97c03b1f6fe0657e91d25f6b7dd1311d6023271a1bcd35978a94a175");
const TV1_INPUT_HASH: [u8; 32] =
    hex!("38a1c8379cceb0fbebfdf3016707e54a1c7e9d21afb9489b9cc58f6055cc9411");
const TV1_SHARED_SECRET: [u8; 32] =
    hex!("d3ac1e8f651a73d2e20b43cb73fd6997de5504afbc04a2d4546a92d0020ba2c6");
const TV1_PUZZLE_HASH: [u8; 32] =
    hex!("23adba149dd9000d65e0f8e21b6975364cbe89a63caf56533df4b7664c21fbf5");

fn tv1_send(k: u32) -> Sent {
    let keys = recipient();
    send(
        &[sender_synthetic_sk(0)],
        &[TV1_COIN_ID.into()],
        keys.scan_pk(),
        keys.spend_pk(),
        k,
    )
}

#[test]
fn tv1_single_output_payment() {
    assert_eq!(sha256(b"test-vector-1-coin"), TV1_COIN_ID);

    let sent = tv1_send(0);
    assert_eq!(sent.input_hash, TV1_INPUT_HASH);
    assert_eq!(
        sent.ecdh_point,
        hex!(
            "aa15516b2b572ebedcd3c048c07189485f8374449923389534125e9976384570"
            "0d6d84d8edaf73b6516874d9a798de09"
        )
    );
    assert_eq!(sent.shared_secret, TV1_SHARED_SECRET);
    assert_eq!(
        sent.tweak,
        hex!("5c560301c50fa309ad43d0f82cd1af143f6e3769659c80e8c14a072331582ab1")
    );
    assert_eq!(
        sent.onetime_pk,
        hex!(
            "b671487c1d275842f529f7a73a63a32a9a1a49e1dbabcac4058cc48626b6db31"
            "f48dc49e769a6f8076a9111ff14e964d"
        )
    );
    assert_eq!(sent.puzzle_hash, Bytes32::from(TV1_PUZZLE_HASH));
    assert_eq!(
        txch(sent.puzzle_hash),
        "txch1ywkm59yamyqq6e0qlr3pk6t4xextazdx8jh4v5ea7jmkvnppl06swd5m0t"
    );

    // Verification: the scanner, using b_scan and A, produces the same puzzle
    // hash, and the recipient's one-time key controls it.
    let keys = recipient();
    let point = tweak_point(&pk(A_0), TV1_INPUT_HASH);
    assert_eq!(
        compute_shared_secret_from_tweak(keys.scan_sk(), &point),
        TV1_SHARED_SECRET
    );
    let detections = scan(&keys, vec![point], vec![output(sent.puzzle_hash, 1)], None);
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].k, 0);
    assert_eq!(detections[0].label, None);

    let onetime_sk = detections[0].onetime_sk(keys.spend_sk());
    assert_eq!(
        onetime_sk.to_bytes(),
        hex!("3c399c61ae130724903b3b650e936ff042b7646764289a33519e17100a89db37")
    );
    assert_eq!(onetime_sk.public_key().to_bytes(), sent.onetime_pk);
}

// ─── Test Vector 2: Multi-Output Payment ────────────────────────────────────

const TV2_COIN_ID: [u8; 32] =
    hex!("b75c75c4787bade82b417272eff88ed90b3013a14b06c16be66b944856f378a2");
const TV2_B_SCAN_PK_B: [u8; 48] = hex!(
    "904b64222fcc0bcf254bcfadcd579cf0530b4fba7ed454f3e6d85799cc9f5491"
    "3f048f1fb393e4acf1bbe56d09d73108"
);
const TV2_B_SPEND_PK_B: [u8; 48] = hex!(
    "99c454a391281b0c0c25ca8175d93ba9d6c4ce9dabe5a25e28b38c2e9ce66aab"
    "e50a73f64a477b212ce110dac1e79813"
);

/// Recipient B's secret keys. The CHIP gives only B's public keys, so the
/// scanner-side verification for B needs secret keys from elsewhere: these are
/// the keys the reference implementation used to produce the vector (unhardened
/// derivation at `m/12381/8444/12/0` and `m/12381/8444/13/0` from the mnemonic
/// below). The test checks that they match the public keys in the CHIP.
fn tv2_recipient_b() -> SilentPaymentKeys {
    let seed = Mnemonic::parse("zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong")
        .unwrap()
        .to_seed("");
    let master = SecretKey::from_seed(&seed);
    let derive = |purpose: u32| {
        [12381, 8444, purpose, 0]
            .into_iter()
            .fold(master.clone(), |key, step| key.derive_unhardened(step))
    };
    SilentPaymentKeys::from_secret_keys(derive(12), derive(13))
}

#[test]
fn tv2_multi_output_payment() {
    assert_eq!(sha256(b"test-vector-2-coin"), TV2_COIN_ID);

    let a = recipient();
    let b = tv2_recipient_b();
    assert_eq!(b.scan_pk().to_bytes(), TV2_B_SCAN_PK_B);
    assert_eq!(b.spend_pk().to_bytes(), TV2_B_SPEND_PK_B);

    let sender = [sender_synthetic_sk(0)];
    let coin_ids = [Bytes32::from(TV2_COIN_ID)];

    // Each recipient is the first (and only) entry of its scan key group, so
    // both outputs use k = 0. The input hash is shared.
    let to_a = send(&sender, &coin_ids, a.scan_pk(), a.spend_pk(), 0);
    let to_b = send(&sender, &coin_ids, b.scan_pk(), b.spend_pk(), 0);

    let input_hash = hex!("42b39c642d50849aa93f23c34085d80bb6a236cad4e0edd2841d526838c92b22");
    assert_eq!(to_a.input_hash, input_hash);
    assert_eq!(to_b.input_hash, input_hash);

    // Recipient A.
    assert_eq!(
        to_a.shared_secret,
        hex!("e9b20a7df882357c76abb4b5f87dcfc9a64cda6413c85f61efa1c4e81e1be50d")
    );
    assert_eq!(
        to_a.tweak,
        hex!("13682ff0957fce11761842863c1658c4cfe9dad4b312fb5fcd71f66567d33d9b")
    );
    assert_eq!(
        to_a.onetime_pk,
        hex!(
            "97b332699dfd7741b3f0c8bf1e1a0edef3b4ad7a092a8f38d74f17216cfb1d0f"
            "5abe7d853f8e3223407be827c9c3aaa8"
        )
    );
    assert_eq!(
        to_a.puzzle_hash,
        Bytes32::from(hex!(
            "596275d286042c639d97e3765fe89d0e5554ac250e0fb4c594a9165017c0a9e5"
        ))
    );
    assert_eq!(
        txch(to_a.puzzle_hash),
        "txch1t938t55xqskx88vhudm9l6yape24ftp9pc8mf3v54yt9q97q48jsqly0xr"
    );

    // Recipient B.
    assert_eq!(
        to_b.shared_secret,
        hex!("1450c748a04e4925bf34d0ef09e21fd1dab0eb0a3675efcb5b2da66d813c06e9")
    );
    assert_eq!(
        to_b.tweak,
        hex!("21d2901f3189dc8def5c5a29e84933a5543ceabd131653dd2ea1951523045976")
    );
    assert_eq!(
        to_b.onetime_pk,
        hex!(
            "a2557b2b6029fcc6783e8447588311a06b5d3dcad132a318d40bf0d6114595dd"
            "3d14fd58fe6f6c88dfa190aaa6bef873"
        )
    );
    assert_eq!(
        to_b.puzzle_hash,
        Bytes32::from(hex!(
            "65249bcb907c9a6fac2e14499f6220cc24ba9359767d680c19405da06b69b263"
        ))
    );
    assert_eq!(
        txch(to_b.puzzle_hash),
        "txch1v5jfhjus0jdxltpwz3ye7c3qesjt4y6ewe7ksrqegpw6q6mfkf3s0xpfcj"
    );

    // Verification.
    assert_ne!(to_a.shared_secret, to_b.shared_secret);
    assert_ne!(to_a.puzzle_hash, to_b.puzzle_hash);

    let point = tweak_point(&pk(A_0), input_hash);
    let outputs = vec![output(to_a.puzzle_hash, 1), output(to_b.puzzle_hash, 2)];

    let found_by_a = scan(&a, vec![point], outputs.clone(), None);
    assert_eq!(found_by_a.len(), 1);
    assert_eq!(found_by_a[0].puzzle_hash, to_a.puzzle_hash);

    let found_by_b = scan(&b, vec![point], outputs, None);
    assert_eq!(found_by_b.len(), 1);
    assert_eq!(found_by_b[0].puzzle_hash, to_b.puzzle_hash);
}

// ─── Test Vector 3: Labeled Payment ─────────────────────────────────────────

#[test]
fn tv3_labeled_payment() {
    let coin_id = hex!("4504f59ea184be18924f95244649287382ec6cdc13f333a8990f648c803a6dac");
    assert_eq!(sha256(b"test-vector-3-coin"), coin_id);

    let keys = recipient();

    // Label generation.
    let (label_scalar, label_pk) = generate_label(keys.scan_sk(), 1);
    assert_eq!(
        label_scalar.to_bytes(),
        hex!("48fa440acca87f501b9984b5d23327d0b7766a4baa913dfb3001d412c48ce465")
    );
    assert_eq!(
        label_pk.to_bytes(),
        hex!(
            "a6dcff3646739745ef7f3ba8e51808dac13765fa9d5e73386d3fbd7841e0773e"
            "02a0f8d91baf57d337954322bd06d80c"
        )
    );
    let labeled = keys
        .labeled_address(SilentPaymentNetwork::Testnet, 1)
        .unwrap();
    assert_eq!(labeled.scan_pk, *keys.scan_pk());
    assert_eq!(
        labeled.spend_pk.to_bytes(),
        hex!(
            "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e962"
            "77d1f02fbbe01cee412dd2ce1b7414c2"
        )
    );

    // Protocol execution: the sender uses (B_scan, B_m).
    let sent = send(
        &[sender_synthetic_sk(0)],
        &[coin_id.into()],
        &labeled.scan_pk,
        &labeled.spend_pk,
        0,
    );
    let input_hash = hex!("58a1875602949aa6bfaf9cb4837957e7175ffb0b14422dbc8d371799f98e66f5");
    assert_eq!(sent.input_hash, input_hash);
    assert_eq!(
        sent.shared_secret,
        hex!("3d1eabb622c40142d4b2557fc222a22cd93d98550255cecb2b6a84985f49215d")
    );
    let t_0 = hex!("301e842ace534f7de854dcc5a48a656d7e9a6d8b8f93db9fb8277f4d1889bdf1");
    assert_eq!(sent.tweak, t_0);
    assert_eq!(
        sent.onetime_pk,
        hex!(
            "97e7466509081a3ed6e50ba0231a6fa1b48d8c910ac6ec933e26cd5091569c61"
            "5f299726c91a730dbf51a26cb249f17c"
        )
    );
    assert_eq!(
        sent.puzzle_hash,
        Bytes32::from(hex!(
            "ba271d218d487e8e5dc994a09a8580e1e8a0559a615bd5805cff11b5a343441c"
        ))
    );
    assert_eq!(
        txch(sent.puzzle_hash),
        "txch1hgn36gvdfplguhwfjjsf4pvqu852q4v6v9datqzulugmtg6rgswqpr9rsm"
    );

    // Label detection: the base one-time key (B_spend, not B_m) does not match
    // the output; adding the label public key does.
    let tweak = ScalarField::from_bytes_raw(t_0);
    let unlabeled_pk = derive_onetime_pk(keys.spend_pk(), &tweak);
    assert_ne!(puzzle_hash_for_pk(&unlabeled_pk), sent.puzzle_hash);
    assert_eq!(
        puzzle_hash_for_pk(&(unlabeled_pk + &label_pk)),
        sent.puzzle_hash
    );

    let mut labels = LabelRegistry::new();
    labels.register(keys.scan_sk(), 1);
    let detections = scan(
        &keys,
        vec![tweak_point(&pk(A_0), input_hash)],
        vec![output(sent.puzzle_hash, 3)],
        Some(&labels),
    );
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].k, 0);
    assert_eq!(detections[0].label, Some(1));

    // Labeled spending key derivation.
    let base_sk = derive_onetime_sk(keys.spend_sk(), &tweak);
    assert_eq!(
        base_sk.to_bytes(),
        hex!("10021d8ab756b398cb4c4732864c264981e39a898e1ff4ea487b8f39f1bb6e77")
    );
    let labeled_sk = detections[0].onetime_sk(keys.spend_sk());
    assert_eq!(
        labeled_sk.to_bytes(),
        hex!("58fc619583ff32e8e6e5cbe8587f4e1a395a04d538b132e5787d634cb64852dc")
    );
    assert_eq!(labeled_sk.public_key().to_bytes(), sent.onetime_pk);
}

// ─── Test Vector 4: Multi-Input Payment ─────────────────────────────────────

#[test]
fn tv4_multi_input_payment() {
    let coin_id_0 = hex!("2b9857e0307ebfbe51829e3be8c992ae57f6a8debe06a5deab429ddae83a8c1a");
    let coin_id_1 = hex!("209bb03a4cd165785e6149bc6dcb27e35829006f02ec927ab5a20521fd27d21a");
    assert_eq!(sha256(b"test-vector-4-coin-0"), coin_id_0);
    assert_eq!(sha256(b"test-vector-4-coin-1"), coin_id_1);

    let sender = [sender_synthetic_sk(0), sender_synthetic_sk(1)];

    // Key aggregation.
    let a_sum = aggregate_sender_sks(&sender).unwrap();
    assert_eq!(
        a_sum.to_bytes(),
        hex!("5600d8781de32f0f3d86bc96b46a3a4ea56ae26da168b55dc66b503acbf95b89")
    );
    let a_sum_pk = hex!(
        "a223ab27f801044cd98c8314014b8073347b0e5aae43c69b78b5ca2a562ee9f7"
        "99b8efad179b34da1b306ca4d62bad40"
    );
    assert_eq!(sk(a_sum.to_bytes()).public_key().to_bytes(), a_sum_pk);
    assert_eq!((pk(A_0) + &pk(A_1)).to_bytes(), a_sum_pk);

    // The input hash is computed from both coin ids; the smaller one (coin 1)
    // is selected whatever the order.
    assert!(coin_id_1 < coin_id_0);
    let input_hash = hex!("3f1071552b7f2f5e49b68166cb204f0a1b6a23b0c30a28bcba59a9c3f766e166");
    for coin_ids in [[coin_id_0, coin_id_1], [coin_id_1, coin_id_0]] {
        let coin_ids = coin_ids.map(Bytes32::from);
        assert_eq!(
            compute_input_hash(&coin_ids, &pk(a_sum_pk)).to_bytes(),
            input_hash
        );
    }
    assert_eq!(
        compute_input_hash(&[coin_id_1.into()], &pk(a_sum_pk)).to_bytes(),
        input_hash
    );
    assert_ne!(
        compute_input_hash(&[coin_id_0.into()], &pk(a_sum_pk)).to_bytes(),
        input_hash
    );

    let keys = recipient();
    let sent = send(
        &sender,
        &[coin_id_0.into(), coin_id_1.into()],
        keys.scan_pk(),
        keys.spend_pk(),
        0,
    );
    assert_eq!(sent.input_hash, input_hash);
    assert_eq!(
        sent.ecdh_point,
        hex!(
            "b99921528f3e0b744040ad552d2209eae9092542f4968ab7a85a5c68098f0924"
            "1a5d5112916f232fdd0edca5f0045080"
        )
    );
    assert_eq!(
        sent.shared_secret,
        hex!("e729dea8c4732747d0e5e930607c52ddfce01ff7c72eaec9ee7c84131e078494")
    );
    assert_eq!(
        sent.tweak,
        hex!("18fafd6001bef3fece078f469731b40a5f362994795f9ff6b9339aa235fee312")
    );
    assert_eq!(
        sent.onetime_pk,
        hex!(
            "b71f484e6d90a657b215ad7bff6f96a8d9bff07e0133d74917cc6c3ef6fa273a"
            "706aa56e1fd6da19ed5466f16450ccb1"
        )
    );
    assert_eq!(
        sent.puzzle_hash,
        Bytes32::from(hex!(
            "5d7fc7d7447c746cfb400e801a169fc7bfd1c13e03bc7866e6b743860a53ac6b"
        ))
    );
    assert_eq!(
        txch(sent.puzzle_hash),
        "txch1t4lu046y036xe76qp6qp595lc7larsf7qw78sehxkapcvzjn434s43wctu"
    );

    // Verification: the scanner, using b_scan and A_sum, finds the output.
    let detections = scan(
        &keys,
        vec![tweak_point(&pk(a_sum_pk), input_hash)],
        vec![output(sent.puzzle_hash, 4)],
        None,
    );
    assert_eq!(detections.len(), 1);
    let onetime_sk = detections[0].onetime_sk(keys.spend_sk());
    assert_eq!(
        onetime_sk.to_bytes(),
        hex!("6ccc3e13145fd561e438d1bb82954cebb63cfa9577ea15404987aa8e0f309399")
    );
    assert_eq!(onetime_sk.public_key().to_bytes(), sent.onetime_pk);
}

// ─── Test Vector 5: Address Encoding ────────────────────────────────────────

#[test]
fn tv5_address_encoding() {
    let keys = recipient();
    let cases = [
        (
            keys.unlabeled_address(SilentPaymentNetwork::Mainnet),
            "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsmqrz7f",
        ),
        (
            keys.unlabeled_address(SilentPaymentNetwork::Testnet),
            "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uws0zz572",
        ),
        (
            keys.labeled_address(SilentPaymentNetwork::Mainnet, 1)
                .unwrap(),
            "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cg8n8kqj",
        ),
        (
            keys.labeled_address(SilentPaymentNetwork::Testnet, 1)
                .unwrap(),
            "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cgn3xqq3",
        ),
    ];

    for (address, expected) in cases {
        assert_eq!(address.encode().unwrap(), expected);
        assert_eq!(SilentPaymentAddress::decode(expected).unwrap(), address);

        // 167 characters on mainnet, 168 on testnet, and the character after
        // the `1` separator is `q`, the version 0 character.
        let (hrp, data) = expected.split_once('1').unwrap();
        assert_eq!(hrp, address.network.hrp());
        assert_eq!(expected.len(), 161 + hrp.len() + 1);
        assert!(data.starts_with('q'));
    }
}

// ─── Test Vector 6: Two Outputs to One Recipient ────────────────────────────

#[test]
fn tv6_two_outputs_to_one_recipient() {
    let keys = recipient();

    // The same recipient listed twice: k = 0 is the output of Test Vector 1,
    // and input_hash and shared_secret are unchanged.
    let first = tv1_send(0);
    let second = tv1_send(1);
    assert_eq!(first.puzzle_hash, Bytes32::from(TV1_PUZZLE_HASH));
    assert_eq!(second.input_hash, TV1_INPUT_HASH);
    assert_eq!(second.shared_secret, TV1_SHARED_SECRET);

    let t_1 = hex!("5b41459e4302fc14258a5ab9af5ac6b45946e83f5a2dadc031b3cb49e79fd899");
    assert_eq!(second.tweak, t_1);
    assert_eq!(
        second.onetime_pk,
        hex!(
            "97c5bddfec949ab9d30190a9ba4d64d12b47828ee2226e1556161c4082344fa9"
            "dd02849f93aa4cdb613e5522bbd08baf"
        )
    );
    assert_eq!(
        second.puzzle_hash,
        Bytes32::from(hex!(
            "0dcde144401307ca3caa58239572e6ab3d0a7f09b4f5d8993d652081c2151f84"
        ))
    );
    assert_eq!(
        txch(second.puzzle_hash),
        "txch1phx7z3zqzvru5092tq3e2uhx4v7s5lcfkn6a3xfav5sgrss4r7zqc2tmh6"
    );
    let onetime_sk_1 = hex!("3b24defe2c06602f0881c526911c87905c90153d58b9c70ac207db36c0d1891f");
    assert_eq!(
        derive_onetime_sk(keys.spend_sk(), &ScalarField::from_bytes_raw(t_1)).to_bytes(),
        onetime_sk_1
    );

    // Verification: a scanner finds k = 0, continues, finds k = 1, and stops
    // at k = 2.
    let point = tweak_point(&pk(A_0), TV1_INPUT_HASH);
    let third = tv1_send(2);
    let detections = scan(
        &keys,
        vec![point],
        vec![output(second.puzzle_hash, 2), output(first.puzzle_hash, 1)],
        None,
    );
    let found: Vec<(u32, Bytes32)> = detections.iter().map(|d| (d.k, d.puzzle_hash)).collect();
    assert_eq!(found, vec![(0, first.puzzle_hash), (1, second.puzzle_hash)]);
    assert_eq!(
        detections[1].onetime_sk(keys.spend_sk()).to_bytes(),
        onetime_sk_1
    );
    assert!(!found.iter().any(|(_, ph)| *ph == third.puzzle_hash));

    // Verification: if the k = 0 output is left out of the transaction, the
    // scanner finds nothing.
    let detections = scan(
        &keys,
        vec![point],
        vec![output(second.puzzle_hash, 2)],
        None,
    );
    assert!(detections.is_empty());
}

// ─── Test Vector 7: Change Output (Label m = 0) ─────────────────────────────

#[test]
fn tv7_change_output() {
    let coin_id = hex!("10f36babd97f3da5027238f336ac15a0f12dfb10bc77191711de7eafba964c9f");
    assert_eq!(sha256(b"test-vector-7-coin"), coin_id);

    let keys = recipient();

    let (label_scalar, label_pk) = generate_label(keys.scan_sk(), 0);
    assert_eq!(
        label_scalar.to_bytes(),
        hex!("3106829938a8b73a652a9a31c6c76a37e32f67f924a50e3649291d6904f22082")
    );
    assert_eq!(
        label_pk.to_bytes(),
        hex!(
            "8314ad1fd7b1dc75d97e025a6d9af28af6ed21e11093a1103ea338c7e496d133"
            "d4f2bd863b47b9a2839ef1ff2d0e6a9f"
        )
    );
    let change = keys.change_address(SilentPaymentNetwork::Testnet);
    assert_eq!(
        change.spend_pk.to_bytes(),
        hex!(
            "a2c089434a6abae657b8e3a868f3d1b94299b141f3da6a6788f966b0856d6802"
            "2bd58459d964b7514111529647ebd7e8"
        )
    );

    // The recipient of Test Vector 1 sends change to itself; the sender keys
    // are those of Test Vector 1.
    let sent = send(
        &[sender_synthetic_sk(0)],
        &[coin_id.into()],
        &change.scan_pk,
        &change.spend_pk,
        0,
    );
    let input_hash = hex!("620c82f4fd9faf8a41d7ed8025dbda3857797c0bc5f31f4c34e460ddc4a89e79");
    assert_eq!(sent.input_hash, input_hash);
    assert_eq!(
        sent.shared_secret,
        hex!("6b13c43195ded3d7e7366decb4c744e371b98a8cf757a9f9cbd784c644d079dc")
    );
    assert_eq!(
        sent.tweak,
        hex!("146540ede99c556f61907c7d0ae1e365aa91f3c116709ef64135a4ccecec0322")
    );
    assert_eq!(
        sent.onetime_pk,
        hex!(
            "b51abe5a989379591b02288a376668ef4124f16b2a5629ba47bebcd4b2359af4"
            "c31428efa13f4bf4ac35ed0dd4805da0"
        )
    );
    assert_eq!(
        sent.puzzle_hash,
        Bytes32::from(hex!(
            "6e0d9f029ea7e8129bf4839d7e805dac5da5b9af5f51fe0374547ac24f8f5257"
        ))
    );
    assert_eq!(
        txch(sent.puzzle_hash),
        "txch1dcxe7q575l5p9xl5swwhaqza43w6twd0tagluqm523avynu02ftsujnl22"
    );

    // Verification: the unlabeled candidate at k = 0 is a different puzzle
    // hash, which is not on chain.
    let unlabeled = puzzle_hash_for_pk(&derive_onetime_pk(
        keys.spend_pk(),
        &ScalarField::from_bytes_raw(sent.tweak),
    ));
    assert_eq!(
        unlabeled,
        Bytes32::from(hex!(
            "980d14e591ef9db6d449eae11c7c43b3f753f07c79da105a06f27f75a2384dc1"
        ))
    );

    // Verification: a scanner with label 0 in its label set (this scanner
    // always has it) finds the output and reports label 0.
    let detections = scan(
        &keys,
        vec![tweak_point(&pk(A_0), input_hash)],
        vec![output(sent.puzzle_hash, 7)],
        None,
    );
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].label, Some(0));
    let onetime_sk = detections[0].onetime_sk(keys.spend_sk());
    assert_eq!(
        onetime_sk.to_bytes(),
        hex!("254f5ce70b4870c4a9b2811bb36b0e79910a88b839a1c6771ab2d222cb0fd42a")
    );
    assert_eq!(onetime_sk.public_key().to_bytes(), sent.onetime_pk);
}
