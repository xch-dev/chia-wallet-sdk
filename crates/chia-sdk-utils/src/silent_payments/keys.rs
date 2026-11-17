//! The recipient's scan and spend keys and their addresses.

use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_sdk_types::silent_payments::{SCAN_PATH, SPEND_PATH};

use super::{
    LabelRegistry, SilentPaymentAddress, SilentPaymentError, SilentPaymentNetwork,
    labels::{CHANGE_LABEL, generate_label},
};

/// A recipient's scan and spend secret keys. The scan key detects payments and the spend key
/// spends them; whoever holds the scan key sees every payment to the address.
///
/// The keys are not wiped from memory on drop.
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
    /// Derives the keys from a mnemonic with an empty passphrase, hardened at every level of
    /// `m/12381n/8444n/12n/0n` (scan) and `m/12381n/8444n/13n/0n` (spend), as CHIP-0057 "Key
    /// Derivation" requires, so the scan key reveals nothing about the spend or master key.
    #[must_use]
    pub fn from_mnemonic(mnemonic: &Mnemonic) -> Self {
        let seed = mnemonic.to_seed("");
        let master = SecretKey::from_seed(&seed);
        let scan_sk = derive_path(&master, SCAN_PATH);
        let spend_sk = derive_path(&master, SPEND_PATH);
        Self::from_secret_keys(scan_sk, spend_sk)
    }

    /// The keys from explicit secret keys. Only keys derived as in [`Self::from_mnemonic`] can
    /// be recovered from the mnemonic by another wallet.
    #[must_use]
    pub fn from_secret_keys(scan_sk: SecretKey, spend_sk: SecretKey) -> Self {
        Self {
            scan_pk: scan_sk.public_key(),
            spend_pk: spend_sk.public_key(),
            scan_sk,
            spend_sk,
        }
    }

    /// The scan secret key.
    #[must_use]
    pub fn scan_sk(&self) -> &SecretKey {
        &self.scan_sk
    }

    /// The spend secret key.
    #[must_use]
    pub fn spend_sk(&self) -> &SecretKey {
        &self.spend_sk
    }

    /// The scan public key.
    #[must_use]
    pub fn scan_pk(&self) -> &PublicKey {
        &self.scan_pk
    }

    /// The unlabeled spend public key.
    #[must_use]
    pub fn spend_pk(&self) -> &PublicKey {
        &self.spend_pk
    }

    /// The unlabeled address on `network`.
    #[must_use]
    pub fn unlabeled_address(&self, network: SilentPaymentNetwork) -> SilentPaymentAddress {
        SilentPaymentAddress::new(self.scan_pk, self.spend_pk, network)
    }

    /// The address of label `m >= 1`. Label 0 is the change label and is rejected; see
    /// [`Self::change_address`].
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

    /// The wallet's own change address, with label 0 (CHIP-0057 "Change Detection"). Never
    /// share it: a payment to it from anyone else would be reported as the wallet's change.
    #[must_use]
    pub fn change_address(&self, network: SilentPaymentNetwork) -> SilentPaymentAddress {
        self.address_for_label(network, CHANGE_LABEL)
    }

    /// A [`LabelRegistry`] of this scan key with `labels` registered.
    #[must_use]
    pub fn label_registry(&self, labels: impl IntoIterator<Item = u32>) -> LabelRegistry {
        let mut registry = LabelRegistry::new(&self.scan_sk);
        for m in labels {
            registry
                .register(&self.scan_sk, m)
                .expect("the registry was created for this scan key");
        }
        registry
    }

    fn address_for_label(&self, network: SilentPaymentNetwork, m: u32) -> SilentPaymentAddress {
        let (_scalar, label_pk) = generate_label(&self.scan_sk, m);
        let labeled_spend_pk = &self.spend_pk + &label_pk;
        SilentPaymentAddress::new(self.scan_pk, labeled_spend_pk, network)
    }
}

/// Derives `path` from `sk`, hardened at every level.
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
