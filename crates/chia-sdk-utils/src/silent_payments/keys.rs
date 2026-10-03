//! `SilentPaymentKeys` — CHIP-0057 scan/spend key derivation and labeled
//! address generation.

use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH};

use super::{
    SilentPaymentAddress, SilentPaymentError, SilentPaymentNetwork,
    labels::{CHANGE_LABEL, generate_label},
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
    /// Derive `(scan_sk, spend_sk)` from a BIP-39 mnemonic with EIP-2333
    /// hardened derivation at every level of the CHIP-0057 paths
    /// `m/12381n/8444n/12n/0n` (scan) and `m/12381n/8444n/13n/0n` (spend), as
    /// CHIP-0057 "Key Derivation" requires.
    ///
    /// Hardened derivation is what makes the scan key safe to place on an
    /// online device: it reveals nothing about the spend key or the master
    /// key, even to someone who also holds the wallet's master public key.
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

    /// Construct from explicit scan and spend secret keys.
    ///
    /// The protocol treats the key pair as given (CHIP-0057 "Key
    /// Derivation"): keys obtained in any way produce valid addresses and
    /// payments, but only keys derived as in [`Self::from_mnemonic`] can be
    /// recovered from a mnemonic by other wallets.
    ///
    /// A watch-only scanner, which holds no spend secret key, does not use
    /// this type; it calls the scanner with the scan secret key and the spend
    /// public key directly.
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
    /// `m = 0` is the reserved change label (CHIP-0057 "Change Detection")
    /// and is rejected here, so that the change address is never handed out by
    /// accident; the wallet obtains it with [`Self::change_address`]. Use
    /// `m >= 1` for labels that are handed out.
    pub fn labeled_address(
        &self,
        network: SilentPaymentNetwork,
        m: u32,
    ) -> Result<SilentPaymentAddress, SilentPaymentError> {
        if m == CHANGE_LABEL {
            return Err(SilentPaymentError::ReservedChangeLabel);
        }
        Ok(self.address_for_label(network, m))
    }

    /// The wallet's own change address: `(B_scan, B_0)`, using the reserved
    /// change label `m = 0` (CHIP-0057 "Change Detection").
    ///
    /// **Never share this address.** It exists so that the wallet can send
    /// change back to itself and recognize it as change when scanning (the
    /// scanner always checks label 0 and reports it as `Some(0)`). If anyone
    /// else learned it, they could create payments that the wallet would
    /// wrongly identify as its own change. Use
    /// [`Self::unlabeled_address`] or [`Self::labeled_address`] for addresses
    /// that are handed out.
    #[must_use]
    pub fn change_address(&self, network: SilentPaymentNetwork) -> SilentPaymentAddress {
        self.address_for_label(network, CHANGE_LABEL)
    }

    fn address_for_label(&self, network: SilentPaymentNetwork, m: u32) -> SilentPaymentAddress {
        let (_scalar, label_pk) = generate_label(&self.scan_sk, m);
        let labeled_spend_pk = &self.spend_pk + &label_pk;
        SilentPaymentAddress::new(self.scan_pk, labeled_spend_pk, network)
    }
}

/// Walk a path with EIP-2333 hardened derivation at every level, as CHIP-0057
/// "Key Derivation" requires for the scan and spend keys.
fn derive_path(sk: &SecretKey, path: &[u32]) -> SecretKey {
    let mut out = sk.clone();
    for &index in path {
        out = out.derive_hardened(index);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // BIP-39 test mnemonic, used by CHIP-0057 Test Vector 8.
    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // CHIP-0057 Test Vector 1. The recipient's scan and spend secret keys are
    // GIVEN values in vectors 1 through 7; they are not derived from a mnemonic.
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

    // TV1 unlabeled mainnet address (CHIP-0057 Test Vector 5).
    const TV1_MAINNET_ADDR: &str = "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsmqrz7f";

    // CHIP-0057 Test Vector 8: hardened derivation from the test mnemonic.
    const TV8_MASTER_SK: [u8; 32] =
        hex!("11da8b4a2874a49dc42984b6aa127b68ef73adddc333319c36fd0446705204a9");
    const TV8_MASTER_PK: [u8; 48] = hex!(
        "82ae65efe846b15a92c51b7ad6c32589fd79d38263d3cbefbeeba08be8e90d8b"
        "c335a1e2fcc66a10b8c817c06232285a"
    );
    const TV8_SCAN_SK: [u8; 32] =
        hex!("0c474f92e8945069c200bb09302d1e569a9b52f59cc04a27874b1bca2adeca9f");
    const TV8_SCAN_PK: [u8; 48] = hex!(
        "8bf2be64f401ccc10d3495503ac9e5b675c2e2b08b073917935beed951b70b8c"
        "cd01f60c406c4ee7e79237e2eb5affd9"
    );
    const TV8_SPEND_SK: [u8; 32] =
        hex!("4f8acf271744cf7050197623569e1603b0193f84b958b5d5f1ce8fd18c908e7b");
    const TV8_SPEND_PK: [u8; 48] = hex!(
        "ad0c7b65a3392b62782b8c0784b10c64b0f4dc8fc2d46d2d3c7c0e1b899bf183"
        "f76996226ddf9145789d6b0bf3499219"
    );
    const TV8_MAINNET_ADDR: &str = "spxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjryvvu9l2";
    const TV8_TESTNET_ADDR: &str = "tspxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjrycwanlf";

    fn tv1_keys() -> SilentPaymentKeys {
        let scan_sk = SecretKey::from_bytes(&TV1_B_SCAN).expect("TV1 b_scan < r");
        let spend_sk = SecretKey::from_bytes(&TV1_B_SPEND).expect("TV1 b_spend < r");
        SilentPaymentKeys::from_secret_keys(scan_sk, spend_sk)
    }

    // from_secret_keys with the given TV1 keys.

    #[test]
    fn from_secret_keys_tv1_public_keys_match() {
        let keys = tv1_keys();
        assert_eq!(keys.scan_sk().to_bytes(), TV1_B_SCAN);
        assert_eq!(keys.spend_sk().to_bytes(), TV1_B_SPEND);
        assert_eq!(keys.scan_pk().to_bytes(), TV1_B_SCAN_PK);
        assert_eq!(keys.spend_pk().to_bytes(), TV1_B_SPEND_PK);
    }

    #[test]
    fn from_secret_keys_tv1_mainnet_pinned() {
        let addr = tv1_keys()
            .unlabeled_address(SilentPaymentNetwork::Mainnet)
            .encode()
            .unwrap();
        assert_eq!(addr, TV1_MAINNET_ADDR);
    }

    // from_mnemonic against CHIP-0057 Test Vector 8.

    #[test]
    fn tv8_master_key_matches() {
        let mnemonic = Mnemonic::parse(TEST_MNEMONIC).expect("BIP-39 test vector");
        let master = SecretKey::from_seed(&mnemonic.to_seed(""));
        assert_eq!(master.to_bytes(), TV8_MASTER_SK);
        assert_eq!(master.public_key().to_bytes(), TV8_MASTER_PK);
    }

    #[test]
    fn tv8_from_mnemonic_uses_hardened_derivation() {
        let mnemonic = Mnemonic::parse(TEST_MNEMONIC).expect("BIP-39 test vector");
        let keys = SilentPaymentKeys::from_mnemonic(&mnemonic);
        assert_eq!(keys.scan_sk().to_bytes(), TV8_SCAN_SK);
        assert_eq!(keys.scan_pk().to_bytes(), TV8_SCAN_PK);
        assert_eq!(keys.spend_sk().to_bytes(), TV8_SPEND_SK);
        assert_eq!(keys.spend_pk().to_bytes(), TV8_SPEND_PK);
    }

    #[test]
    fn tv8_addresses_match() {
        let mnemonic = Mnemonic::parse(TEST_MNEMONIC).expect("BIP-39 test vector");
        let keys = SilentPaymentKeys::from_mnemonic(&mnemonic);
        assert_eq!(
            keys.unlabeled_address(SilentPaymentNetwork::Mainnet)
                .encode()
                .unwrap(),
            TV8_MAINNET_ADDR
        );
        assert_eq!(
            keys.unlabeled_address(SilentPaymentNetwork::Testnet)
                .encode()
                .unwrap(),
            TV8_TESTNET_ADDR
        );
    }

    /// The mnemonic-derived keys are not the given keys of vectors 1 through 7,
    /// and not what unhardened derivation along the same indices would give.
    #[test]
    fn from_mnemonic_is_not_unhardened() {
        use chia_bls::DerivableKey;

        let mnemonic = Mnemonic::parse(TEST_MNEMONIC).expect("BIP-39 test vector");
        let keys = SilentPaymentKeys::from_mnemonic(&mnemonic);

        let master = SecretKey::from_seed(&mnemonic.to_seed(""));
        let unhardened = SCAN_PATH
            .iter()
            .fold(master, |sk, &index| sk.derive_unhardened(index));
        assert_ne!(keys.scan_sk().to_bytes(), unhardened.to_bytes());
        assert_ne!(keys.scan_sk().to_bytes(), TV1_B_SCAN);
    }

    // labeled_address(0) is rejected as the reserved change label.

    #[test]
    fn labeled_address_zero_rejected() {
        let result = tv1_keys().labeled_address(SilentPaymentNetwork::Mainnet, 0);
        assert_eq!(result, Err(SilentPaymentError::ReservedChangeLabel));
    }
}
