//! Labels and the registry a scanner looks them up in (CHIP-0057 "Label Generation").

use std::collections::{BTreeMap, HashMap};

use chia_bls::{PublicKey, SecretKey};
use chia_sdk_types::silent_payments::{CHIA_SP_LABEL, ScalarField, tagged_hash};

use super::SilentPaymentError;

/// The label reserved for the wallet's own change (CHIP-0057 "Change Detection"). Its address
/// must never be handed out.
pub const CHANGE_LABEL: u32 = 0;

/// The label scalar and label public key for label index `m`, including `m = 0`.
pub(crate) fn generate_label(scan_sk: &SecretKey, m: u32) -> (ScalarField, PublicKey) {
    let mut data = [0u8; 36];
    data[..32].copy_from_slice(&scan_sk.to_bytes());
    data[32..].copy_from_slice(&m.to_be_bytes());

    let hash = tagged_hash(CHIA_SP_LABEL, &data);
    let label_scalar = ScalarField::from_bytes_unsigned(hash);
    let label_sk = SecretKey::from_bytes(label_scalar.as_bytes())
        .expect("ScalarField::from_bytes_unsigned guarantees value < r");
    (label_scalar, label_sk.public_key())
}

/// The labels a wallet scans for under one scan key. It stores the scan public key and no
/// secret; [`LabelRegistry::register`] and the scanner refuse another scan key.
#[derive(Clone, Debug)]
pub struct LabelRegistry {
    scan_pk: PublicKey,
    forward: BTreeMap<u32, PublicKey>,
    reverse: HashMap<[u8; 48], u32>,
}

impl LabelRegistry {
    /// An empty registry for the scan secret key `scan_sk`.
    #[must_use]
    pub fn new(scan_sk: &SecretKey) -> Self {
        Self {
            scan_pk: scan_sk.public_key(),
            forward: BTreeMap::new(),
            reverse: HashMap::new(),
        }
    }

    /// The scan public key the registry belongs to.
    #[must_use]
    pub fn scan_pk(&self) -> &PublicKey {
        &self.scan_pk
    }

    /// Registers label `m`. Returns [`SilentPaymentError::LabelRegistryKeyMismatch`] if
    /// `scan_sk` is not the registry's scan key. Label 0 is always scanned, registered or not.
    pub fn register(&mut self, scan_sk: &SecretKey, m: u32) -> Result<(), SilentPaymentError> {
        if scan_sk.public_key() != self.scan_pk {
            return Err(SilentPaymentError::LabelRegistryKeyMismatch);
        }
        let (_scalar, label_pk) = generate_label(scan_sk, m);
        if let Some(previous) = self.forward.insert(m, label_pk) {
            self.reverse.remove(&previous.to_bytes());
        }
        self.reverse.insert(label_pk.to_bytes(), m);
        Ok(())
    }

    /// The label public key of a registered label.
    #[must_use]
    pub fn forward(&self, m: u32) -> Option<&PublicKey> {
        self.forward.get(&m)
    }

    /// The registered label with the label public key `label_pk`.
    #[must_use]
    pub fn lookup(&self, label_pk: &PublicKey) -> Option<u32> {
        self.reverse.get(&label_pk.to_bytes()).copied()
    }

    /// The number of registered labels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Whether no labels are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// The registered `(m, label_pk)` pairs in ascending order of `m`.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &PublicKey)> {
        self.forward.iter().map(|(&m, pk)| (m, pk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // TV1 pinned bytes (CHIP-0057 test vector 1).
    const TV1_B_SCAN: [u8; 32] =
        hex!("132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6");
    const TV1_B_SPEND: [u8; 32] =
        hex!("53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087");
    const TV1_B_SPEND_PK: [u8; 48] = hex!(
        "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
        "f6f31e22e0831ba5088c952625a75c74"
    );

    // TV3 (CHIP-0057 test vector 3 — m = 1) pinned bytes.
    const TV3_LABEL_SCALAR: [u8; 32] =
        hex!("48fa440acca87f501b9984b5d23327d0b7766a4baa913dfb3001d412c48ce465");
    const TV3_LABEL_PK: [u8; 48] = hex!(
        "a6dcff3646739745ef7f3ba8e51808dac13765fa9d5e73386d3fbd7841e0773e"
        "02a0f8d91baf57d337954322bd06d80c"
    );
    const TV3_B_M: [u8; 48] = hex!(
        "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e962"
        "77d1f02fbbe01cee412dd2ce1b7414c2"
    );

    fn sk(bytes: [u8; 32]) -> SecretKey {
        SecretKey::from_bytes(&bytes).expect("test vector secret key")
    }
    fn pk(bytes: [u8; 48]) -> PublicKey {
        PublicKey::from_bytes(&bytes).expect("test vector public key")
    }

    // ─── TV3 label-scalar pinning ──────────────────────────────────────────

    #[test]
    fn tv3_label_scalar_matches() {
        let (scalar, _) = generate_label(&sk(TV1_B_SCAN), 1);
        assert_eq!(scalar.to_bytes(), TV3_LABEL_SCALAR);
    }

    #[test]
    fn tv3_label_pk_matches() {
        let (_, label_pk) = generate_label(&sk(TV1_B_SCAN), 1);
        assert_eq!(label_pk.to_bytes(), TV3_LABEL_PK);
    }

    // ─── TV3 B_m = B_spend + label_pk ──────────────────────────────────────

    #[test]
    fn tv3_labeled_spend_pk_matches() {
        let spend_pk = pk(TV1_B_SPEND_PK);
        let (_, label_pk) = generate_label(&sk(TV1_B_SCAN), 1);
        let b_m = &spend_pk + &label_pk;
        assert_eq!(b_m.to_bytes(), TV3_B_M);
    }

    // Labeled sub-addresses keep the same scan_pk as the unlabeled address.

    #[test]
    fn labels_preserve_scan_pk() {
        use super::super::{SilentPaymentKeys, SilentPaymentNetwork};
        let keys = SilentPaymentKeys::from_secret_keys(sk(TV1_B_SCAN), sk(TV1_B_SPEND));
        let unlabeled = keys.unlabeled_address(SilentPaymentNetwork::Mainnet);
        for &m in &[1u32, 2, 100] {
            let labeled = keys
                .labeled_address(SilentPaymentNetwork::Mainnet, m)
                .expect("m != 0 should succeed");
            assert_eq!(labeled.scan_pk, unlabeled.scan_pk, "scan_pk drift at m={m}");
        }
    }

    // ─── LabelRegistry round-trip ──────────────────────────────────────────

    #[test]
    fn registry_round_trip() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new(&scan_sk);
        reg.register(&scan_sk, 1).unwrap();
        let label_pk = *reg.forward(1).expect("registered");
        assert_eq!(reg.lookup(&label_pk), Some(1));
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn registry_three_labels() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new(&scan_sk);
        reg.register(&scan_sk, 1).unwrap();
        reg.register(&scan_sk, 2).unwrap();
        reg.register(&scan_sk, 3).unwrap();
        assert_eq!(reg.len(), 3);
        for m in [1u32, 2, 3] {
            let pk = *reg.forward(m).expect("registered");
            assert_eq!(reg.lookup(&pk), Some(m));
        }
    }

    #[test]
    fn registry_lookup_missing() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new(&scan_sk);
        reg.register(&scan_sk, 1).unwrap();
        // A different label index produces a different label_pk by the
        // discrete-log assumption; use generate_label to construct it.
        let (_, other_label_pk) = generate_label(&scan_sk, 999);
        assert_eq!(reg.lookup(&other_label_pk), None);
    }

    #[test]
    fn registry_forward_missing() {
        let reg = LabelRegistry::new(&sk(TV1_B_SCAN));
        assert_eq!(reg.forward(42), None);
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn registry_iterates_in_ascending_label_order() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new(&scan_sk);
        for m in [7u32, 2, 100, 1, 0] {
            reg.register(&scan_sk, m).unwrap();
        }
        let order: Vec<u32> = reg.iter().map(|(m, _)| m).collect();
        assert_eq!(order, vec![0, 1, 2, 7, 100]);
    }

    // ─── CHIP-0057 Test Vector 7: change label (m = 0) ─────────────────────

    const TV7_LABEL_SCALAR: [u8; 32] =
        hex!("3106829938a8b73a652a9a31c6c76a37e32f67f924a50e3649291d6904f22082");
    const TV7_LABEL_PK: [u8; 48] = hex!(
        "8314ad1fd7b1dc75d97e025a6d9af28af6ed21e11093a1103ea338c7e496d133"
        "d4f2bd863b47b9a2839ef1ff2d0e6a9f"
    );
    const TV7_B_0: [u8; 48] = hex!(
        "a2c089434a6abae657b8e3a868f3d1b94299b141f3da6a6788f966b0856d6802"
        "2bd58459d964b7514111529647ebd7e8"
    );

    #[test]
    fn tv7_change_label_matches() {
        let (scalar, label_pk) = generate_label(&sk(TV1_B_SCAN), CHANGE_LABEL);
        assert_eq!(scalar.to_bytes(), TV7_LABEL_SCALAR);
        assert_eq!(label_pk.to_bytes(), TV7_LABEL_PK);
        assert_eq!((&pk(TV1_B_SPEND_PK) + &label_pk).to_bytes(), TV7_B_0);
    }

    /// The wallet's own change address is `(B_scan, B_0)`. It is available only
    /// through the explicitly named `change_address`; `labeled_address(0)`
    /// stays rejected so that it is not handed out by accident.
    #[test]
    fn tv7_change_address() {
        use super::super::{SilentPaymentError, SilentPaymentKeys, SilentPaymentNetwork};
        let keys = SilentPaymentKeys::from_secret_keys(sk(TV1_B_SCAN), sk(TV1_B_SPEND));

        let change = keys.change_address(SilentPaymentNetwork::Mainnet);
        assert_eq!(change.scan_pk, *keys.scan_pk());
        assert_eq!(change.spend_pk.to_bytes(), TV7_B_0);

        assert_eq!(
            keys.labeled_address(SilentPaymentNetwork::Mainnet, CHANGE_LABEL),
            Err(SilentPaymentError::ReservedChangeLabel)
        );
    }

    /// The registry is bound to its scan key: another key is refused, and
    /// registering a label again leaves one entry in each direction.
    #[test]
    fn registry_is_bound_to_its_scan_key() {
        let scan_sk = sk(TV1_B_SCAN);
        let other_sk = sk(TV1_B_SPEND);
        let mut reg = LabelRegistry::new(&scan_sk);
        assert_eq!(*reg.scan_pk(), scan_sk.public_key());

        assert_eq!(
            reg.register(&other_sk, 1),
            Err(SilentPaymentError::LabelRegistryKeyMismatch)
        );
        assert!(reg.is_empty());

        reg.register(&scan_sk, 1).unwrap();
        reg.register(&scan_sk, 1).unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.reverse.len(), 1);
        let (_, other_label_pk) = generate_label(&other_sk, 1);
        assert_eq!(reg.lookup(&other_label_pk), None);
    }
}
