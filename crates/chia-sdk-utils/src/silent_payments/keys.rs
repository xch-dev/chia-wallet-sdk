//! `SilentPaymentKeys` — CHIP-0057 scan/spend key derivation and labeled
//! address generation.

use bip39::Mnemonic;
use chia_bls::{DerivableKey, PublicKey, SecretKey};
use chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH};

use super::{
    SilentPaymentAddress, SilentPaymentError, SilentPaymentNetwork, labels::generate_label,
};

/// The wallet-author-facing key bundle for CHIP-0057 silent payments.
///
/// Stores both the scan and spend BLS secret keys plus their cached public
/// keys (computed once at construction). The scan key is used for incoming-
/// payment detection; the spend key is used to sign coin spends for detected
/// payments.
///
/// **Privacy note:** the scan key bypasses the standard wallet's privacy
/// boundary. Anyone who holds this scan key can see every silent-payment output
/// addressed to the associated address. Wallet authors should treat `scan_sk`
/// as the more sensitive of the two keys for at-rest storage.
///
/// **Lifetime hygiene:** this type does NOT implement `Zeroize`. Matching the
/// SDK norm (`chia_bls::SecretKey` and `chia_sdk_test::BlsPair` also do not),
/// it is the wallet author's responsibility to drop / overwrite the bundle
/// when secret material is no longer needed.
#[derive(Clone)]
pub struct SilentPaymentKeys {
    scan_sk: SecretKey,
    spend_sk: SecretKey,
    scan_pk: PublicKey,
    spend_pk: PublicKey,
}

impl core::fmt::Debug for SilentPaymentKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SilentPaymentKeys")
            .field("scan_pk", &self.scan_pk)
            .field("spend_pk", &self.spend_pk)
            .field("scan_sk", &"<redacted>")
            .field("spend_sk", &"<redacted>")
            .finish()
    }
}

impl SilentPaymentKeys {
    /// Derive `(scan_sk, spend_sk)` from a BIP-39 mnemonic at the CHIP-0057
    /// paths `m/12381/8444/12/0` (scan) and `m/12381/8444/13/0` (spend).
    ///
    /// Uses an empty BIP-39 passphrase — matches the standard Chia wallet
    /// convention (also used by `chia_sdk_test::BlsPair::new` and
    /// `chia_sdk_bindings::Mnemonic`).
    #[must_use]
    pub fn from_mnemonic(mnemonic: &Mnemonic) -> Self {
        let seed = mnemonic.to_seed("");
        let master = SecretKey::from_seed(&seed);
        let scan_sk = derive_path(&master, SCAN_PATH);
        let spend_sk = derive_path(&master, SPEND_PATH);
        Self::from_secret_keys(scan_sk, spend_sk)
    }

    /// Construct from raw scan + spend secret keys. Enables watch-only and
    /// key-import flows where the consumer holds keys derived elsewhere
    /// (e.g., a hardware wallet that splits scan online vs spend offline,
    /// CHIP §10).
    #[must_use]
    pub fn from_secret_keys(scan_sk: SecretKey, spend_sk: SecretKey) -> Self {
        Self {
            scan_pk: scan_sk.public_key(),
            spend_pk: spend_sk.public_key(),
            scan_sk,
            spend_sk,
        }
    }

    /// The scan secret key (`b_scan` in CHIP terminology).
    #[must_use]
    pub fn scan_sk(&self) -> &SecretKey {
        &self.scan_sk
    }

    /// The spend secret key (`b_spend` in CHIP terminology).
    #[must_use]
    pub fn spend_sk(&self) -> &SecretKey {
        &self.spend_sk
    }

    /// The scan public key (`B_scan` in CHIP terminology).
    #[must_use]
    pub fn scan_pk(&self) -> &PublicKey {
        &self.scan_pk
    }

    /// The spend public key (`B_spend` in CHIP terminology — the unlabeled
    /// spend pubkey; labeled sub-addresses use `B_m = B_spend + label_pk`).
    #[must_use]
    pub fn spend_pk(&self) -> &PublicKey {
        &self.spend_pk
    }

    /// Build the unlabeled bech32m silent-payment address for this key bundle
    /// on the given network.
    #[must_use]
    pub fn unlabeled_address(&self, network: SilentPaymentNetwork) -> SilentPaymentAddress {
        SilentPaymentAddress::new(self.scan_pk, self.spend_pk, network)
    }

    /// Build a labeled bech32m silent-payment sub-address.
    ///
    /// `m = 0` is the reserved change label (CHIP §125-§130) and is rejected
    /// at this public boundary — never expose a change address publicly. Use
    /// `m ∈ [1, u32::MAX - 1]` for end-user-facing labels (treat `u32::MAX`
    /// as reserved per the CHIP draft).
    pub fn labeled_address(
        &self,
        network: SilentPaymentNetwork,
        m: u32,
    ) -> Result<SilentPaymentAddress, SilentPaymentError> {
        if m == 0 {
            return Err(SilentPaymentError::ReservedChangeLabel);
        }
        let (_scalar, label_pk) = generate_label(&self.scan_sk, m);
        let labeled_spend_pk = &self.spend_pk + &label_pk;
        Ok(SilentPaymentAddress::new(
            self.scan_pk,
            labeled_spend_pk,
            network,
        ))
    }
}

/// Walk an unhardened BIP-32 path on a chia-bls secret key.
///
/// All indices are unhardened (no `| 0x80000000`) — matches CHIP-0057 §172-173
/// and the `chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH}` constants.
fn derive_path(sk: &SecretKey, path: &[u32]) -> SecretKey {
    let mut out = sk.clone();
    for &index in path {
        out = out.derive_unhardened(index);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // BIP-39 test vector mnemonic — also the CHIP-0057 TV1 mnemonic.
    const TV1_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // TV1 pinned bytes (CHIP-0057 test vector 1).
    const TV1_B_SCAN: [u8; 32] =
        hex!("132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6");
    const TV1_B_SPEND: [u8; 32] =
        hex!("53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087");
    const TV1_B_SCAN_PK: [u8; 48] = hex!(
        "a04f404bfbfdc9311736899fe32d2275bb007814510c3523529487ad75736075"
        "73ade20d31c75107b40331fff79ac896"
    );
    const TV1_B_SPEND_PK: [u8; 48] = hex!(
        "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
        "f6f31e22e0831ba5088c952625a75c74"
    );

    // TV1 unlabeled mainnet address (CHIP-0057 test vector 1).
    const TV1_MAINNET_ADDR: &str = "spxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsfn2u2q";

    fn tv1_keys_from_mnemonic() -> SilentPaymentKeys {
        let mnemonic = Mnemonic::parse(TV1_MNEMONIC).expect("BIP-39 test vector");
        SilentPaymentKeys::from_mnemonic(&mnemonic)
    }

    fn tv1_keys_from_secret_keys() -> SilentPaymentKeys {
        let scan_sk = SecretKey::from_bytes(&TV1_B_SCAN).expect("TV1 b_scan < r");
        let spend_sk = SecretKey::from_bytes(&TV1_B_SPEND).expect("TV1 b_spend < r");
        SilentPaymentKeys::from_secret_keys(scan_sk, spend_sk)
    }

    // from_mnemonic derivation against the CHIP-0057 TV1 byte vectors.

    #[test]
    fn from_mnemonic_tv1_scan_sk_matches() {
        let keys = tv1_keys_from_mnemonic();
        assert_eq!(keys.scan_sk().to_bytes(), TV1_B_SCAN);
    }

    #[test]
    fn from_mnemonic_tv1_spend_sk_matches() {
        let keys = tv1_keys_from_mnemonic();
        assert_eq!(keys.spend_sk().to_bytes(), TV1_B_SPEND);
    }

    #[test]
    fn from_mnemonic_tv1_scan_pk_matches() {
        let keys = tv1_keys_from_mnemonic();
        assert_eq!(keys.scan_pk().to_bytes(), TV1_B_SCAN_PK);
    }

    #[test]
    fn from_mnemonic_tv1_spend_pk_matches() {
        let keys = tv1_keys_from_mnemonic();
        assert_eq!(keys.spend_pk().to_bytes(), TV1_B_SPEND_PK);
    }

    // from_secret_keys produces the same address as from_mnemonic.

    #[test]
    fn from_secret_keys_matches_from_mnemonic() {
        let from_m = tv1_keys_from_mnemonic();
        let from_sk = tv1_keys_from_secret_keys();
        // Pubkeys must match exactly — equivalence of construction paths.
        assert_eq!(from_m.scan_pk(), from_sk.scan_pk());
        assert_eq!(from_m.spend_pk(), from_sk.spend_pk());
        // And the encoded address strings match.
        let from_m_addr = from_m
            .unlabeled_address(SilentPaymentNetwork::Mainnet)
            .encode()
            .unwrap();
        let from_sk_addr = from_sk
            .unlabeled_address(SilentPaymentNetwork::Mainnet)
            .encode()
            .unwrap();
        assert_eq!(from_m_addr, from_sk_addr);
    }

    #[test]
    fn from_secret_keys_tv1_mainnet_pinned() {
        let keys = tv1_keys_from_secret_keys();
        let addr = keys
            .unlabeled_address(SilentPaymentNetwork::Mainnet)
            .encode()
            .unwrap();
        assert_eq!(addr, TV1_MAINNET_ADDR);
    }

    // labeled_address(0) is rejected as the reserved change label.

    #[test]
    fn labeled_address_zero_rejected() {
        let keys = tv1_keys_from_mnemonic();
        let result = keys.labeled_address(SilentPaymentNetwork::Mainnet, 0);
        assert_eq!(result, Err(SilentPaymentError::ReservedChangeLabel));
    }
}
