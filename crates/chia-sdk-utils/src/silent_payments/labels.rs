//! Label generation and label-index ↔ label-pubkey registry for CHIP-0057
//! silent payments.
//!
//! Per CHIP-0057 §125-§130:
//! ```text
//! label_data    = ser256(b_scan) || ser32(m)
//! label_scalar  = int(tagged_hash("Chia_SP/Label", label_data)) mod r
//! label_pk      = label_scalar * G
//! B_m           = B_spend + label_pk
//! ```
//!
//! `m = 0` is the change-label sentinel. It is REJECTED at the public boundary
//! ([`super::SilentPaymentKeys::labeled_address`]) but accepted internally by
//! [`generate_label`] and [`LabelRegistry::register`] because the scanner
//! legitimately needs to register the change label to detect its own change
//! outputs.

use std::collections::HashMap;

use chia_bls::{PublicKey, SecretKey};
use chia_sdk_types::silent_payments::{CHIA_SP_LABEL, ScalarField, tagged_hash};

/// Compute the label scalar and label public key for label index `m`.
///
/// Returns `(label_scalar, label_pk)` where `label_pk = label_scalar * G`.
/// The scalar uses UNSIGNED mod-r reduction via the [`ScalarField`] boundary;
/// see `ScalarField::from_bytes_unsigned` for why that interpretation is
/// mandatory.
///
/// `m = 0` is accepted at this layer — the public boundary is in
/// [`super::SilentPaymentKeys::labeled_address`].
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

/// A bidirectional registry of `label_index ↔ label_pk` mappings.
///
/// Used by the scanner to attribute a labeled detection back to its label index
/// (`lookup` is called inside the k-iteration loop).
///
/// Storage: two `HashMap` entries per registered label. For the realistic
/// upper bound of a few hundred labels per wallet, total memory is well under
/// 100 KB.
#[derive(Clone, Debug, Default)]
pub struct LabelRegistry {
    forward: HashMap<u32, PublicKey>,
    reverse: HashMap<[u8; 48], u32>,
}

impl LabelRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register label `m` against scan secret key `scan_sk`.
    ///
    /// `m = 0` is accepted here — the public-API change-label rejection lives
    /// in [`super::SilentPaymentKeys::labeled_address`]. Scanner code needs the
    /// change label registered internally to detect change outputs.
    pub fn register(&mut self, scan_sk: &SecretKey, m: u32) {
        let (_scalar, label_pk) = generate_label(scan_sk, m);
        let bytes = label_pk.to_bytes();
        self.forward.insert(m, label_pk);
        self.reverse.insert(bytes, m);
    }

    /// Look up the label public key for a registered label index.
    #[must_use]
    pub fn forward(&self, m: u32) -> Option<&PublicKey> {
        self.forward.get(&m)
    }

    /// Look up the label index that registers a given label public key.
    /// Used by the scanner to attribute a labeled detection.
    #[must_use]
    pub fn lookup(&self, label_pk: &PublicKey) -> Option<u32> {
        self.reverse.get(&label_pk.to_bytes()).copied()
    }

    /// Number of registered labels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// True if no labels are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Iterate registered `(m, label_pk)` pairs. Useful for the scanner's
    /// labeled-detection branch.
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
        let mut reg = LabelRegistry::new();
        reg.register(&scan_sk, 1);
        let label_pk = *reg.forward(1).expect("registered");
        assert_eq!(reg.lookup(&label_pk), Some(1));
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn registry_three_labels() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new();
        reg.register(&scan_sk, 1);
        reg.register(&scan_sk, 2);
        reg.register(&scan_sk, 3);
        assert_eq!(reg.len(), 3);
        for m in [1u32, 2, 3] {
            let pk = *reg.forward(m).expect("registered");
            assert_eq!(reg.lookup(&pk), Some(m));
        }
    }

    #[test]
    fn registry_lookup_missing() {
        let scan_sk = sk(TV1_B_SCAN);
        let mut reg = LabelRegistry::new();
        reg.register(&scan_sk, 1);
        // A different label index produces a different label_pk by the
        // discrete-log assumption; use generate_label to construct it.
        let (_, other_label_pk) = generate_label(&scan_sk, 999);
        assert_eq!(reg.lookup(&other_label_pk), None);
    }

    #[test]
    fn registry_forward_missing() {
        let reg = LabelRegistry::new();
        assert_eq!(reg.forward(42), None);
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }
}
