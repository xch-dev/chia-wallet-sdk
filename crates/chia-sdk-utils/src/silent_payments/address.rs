//! Silent-payment bech32m address (HRP `spxch` mainnet / `tspxch` testnet)
//! and the network discriminant.

use chia_bls::PublicKey;
use chia_protocol::Bytes;

use crate::Bech32;

use super::SilentPaymentError;

/// Network discriminator for silent-payment addresses.
///
/// Each network maps to a fixed HRP per CHIP-0057 §153, §206:
/// - `Mainnet` → `"spxch"`
/// - `Testnet` → `"tspxch"`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SilentPaymentNetwork {
    Mainnet,
    Testnet,
}

impl SilentPaymentNetwork {
    /// The bech32m human-readable part for this network.
    #[must_use]
    pub fn hrp(self) -> &'static str {
        match self {
            Self::Mainnet => "spxch",
            Self::Testnet => "tspxch",
        }
    }

    /// Parse a bech32m HRP string into a network discriminator. Returns
    /// `Err(SilentPaymentError::WrongHrp(_))` for any value other than
    /// `"spxch"` or `"tspxch"`.
    pub fn from_hrp(hrp: &str) -> Result<Self, SilentPaymentError> {
        match hrp {
            "spxch" => Ok(Self::Mainnet),
            "tspxch" => Ok(Self::Testnet),
            other => Err(SilentPaymentError::WrongHrp(other.to_string())),
        }
    }
}

/// A CHIP-0057 silent-payment address: bech32m-encoded
/// `serialize(B_scan) || serialize(B_spend)` (96 bytes) under HRP
/// `spxch` (mainnet) / `tspxch` (testnet).
///
/// The address carries no labeled-vs-unlabeled discriminant: per CHIP §375,
/// a labeled address has its `spend_pk` field set to `B_m = B_spend + label_pk`,
/// but the wire form is identical to an unlabeled address with the same
/// underlying point. Senders and scanners cannot distinguish; recipients
/// disambiguate via a registered [`super::LabelRegistry`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SilentPaymentAddress {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub network: SilentPaymentNetwork,
}

impl SilentPaymentAddress {
    /// Construct a `SilentPaymentAddress` from raw pubkeys + network.
    ///
    /// Does NOT validate the pubkeys against the identity element — that
    /// check is performed on `decode` (where untrusted bytes enter the system).
    /// `new` is the trusted constructor used by `SilentPaymentKeys::unlabeled_address`
    /// and `SilentPaymentKeys::labeled_address`, where the pubkeys come from
    /// `SecretKey::public_key()` and are therefore non-identity by construction.
    #[must_use]
    pub fn new(scan_pk: PublicKey, spend_pk: PublicKey, network: SilentPaymentNetwork) -> Self {
        Self {
            scan_pk,
            spend_pk,
            network,
        }
    }

    /// Encode as bech32m: HRP `||` `"1"` `||` base32(`scan_pk` `||` `spend_pk` `||` checksum).
    ///
    /// The payload is the 96-byte concatenation `serialize(B_scan) || serialize(B_spend)`.
    pub fn encode(&self) -> Result<String, SilentPaymentError> {
        let mut payload = Vec::with_capacity(96);
        payload.extend_from_slice(&self.scan_pk.to_bytes());
        payload.extend_from_slice(&self.spend_pk.to_bytes());
        debug_assert_eq!(payload.len(), 96);

        let bech = Bech32::new(Bytes::new(payload), self.network.hrp().to_string());
        Ok(bech.encode()?)
    }

    /// Decode a bech32m silent-payment address.
    ///
    /// Rejects:
    ///   - HRP not in `{"spxch", "tspxch"}` → `SilentPaymentError::WrongHrp`.
    ///   - bech32 / bech32m parse failure → `SilentPaymentError::Bech32(_)`.
    ///   - non-bech32m variant (plain bech32) → `SilentPaymentError::Bech32(Bech32Error::InvalidFormat)`.
    ///   - payload length != 96 → `SilentPaymentError::PayloadLength(N)`.
    ///   - either pubkey half is invalid bytes → `SilentPaymentError::InvalidPublicKey`.
    ///   - either pubkey half is the identity element → `SilentPaymentError::IdentityPublicKey`.
    pub fn decode(s: &str) -> Result<Self, SilentPaymentError> {
        let bech = Bech32::decode(s)?;
        let network = SilentPaymentNetwork::from_hrp(&bech.prefix)?;
        let payload: Vec<u8> = bech.data.to_vec();
        if payload.len() != 96 {
            return Err(SilentPaymentError::PayloadLength(payload.len()));
        }
        let scan_bytes: [u8; 48] = payload[..48].try_into().expect("96 == 48 + 48");
        let spend_bytes: [u8; 48] = payload[48..].try_into().expect("96 == 48 + 48");
        let scan_pk =
            PublicKey::from_bytes(&scan_bytes).map_err(|_| SilentPaymentError::InvalidPublicKey)?;
        let spend_pk = PublicKey::from_bytes(&spend_bytes)
            .map_err(|_| SilentPaymentError::InvalidPublicKey)?;
        if scan_pk.is_inf() || spend_pk.is_inf() {
            return Err(SilentPaymentError::IdentityPublicKey);
        }
        Ok(Self {
            scan_pk,
            spend_pk,
            network,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // ─── TV1 (CHIP-0057 test vector 1) ─────────────────────────────────────
    // BIP-39 test mnemonic: "abandon abandon abandon abandon abandon abandon
    //                       abandon abandon abandon abandon abandon about"
    // SCAN_PATH:  m/12381/8444/12/0
    // SPEND_PATH: m/12381/8444/13/0

    const TV1_SCAN_PK_BYTES: [u8; 48] = hex!(
        "a04f404bfbfdc9311736899fe32d2275bb007814510c3523529487ad75736075"
        "73ade20d31c75107b40331fff79ac896"
    );
    const TV1_SPEND_PK_BYTES: [u8; 48] = hex!(
        "8afc580192f44fab624f613369f792eff3220ea3ca822eb839ab2c9309e527db"
        "f6f31e22e0831ba5088c952625a75c74"
    );

    const TV1_MAINNET_ADDR: &str = "spxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsfn2u2q";
    const TV1_TESTNET_ADDR: &str = "tspxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsnlcstc";

    // ─── TV3 (CHIP-0057 test vector 3 — labeled, m = 1) ────────────────────
    // B_m = B_spend + label_pk; recipient's scan_pk is unchanged from TV1.

    const TV3_B_M_BYTES: [u8; 48] = hex!(
        "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e962"
        "77d1f02fbbe01cee412dd2ce1b7414c2"
    );

    const TV3_MAINNET_LABELED_ADDR: &str = "spxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cg4qwg5m";
    const TV3_TESTNET_LABELED_ADDR: &str = "tspxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cg0vuy4r";

    // Helper: parse a 48-byte hex array into a chia_bls::PublicKey.
    fn pk(bytes: [u8; 48]) -> PublicKey {
        PublicKey::from_bytes(&bytes).expect("test vector pubkey")
    }

    // ─── Positive: TV1 round-trip ──────────────────────────────────────────

    #[test]
    fn tv1_mainnet_round_trip() {
        let decoded = SilentPaymentAddress::decode(TV1_MAINNET_ADDR).unwrap();
        let re = decoded.encode().unwrap();
        assert_eq!(re, TV1_MAINNET_ADDR);
        assert_eq!(decoded.network, SilentPaymentNetwork::Mainnet);
    }

    #[test]
    fn tv1_testnet_round_trip() {
        let decoded = SilentPaymentAddress::decode(TV1_TESTNET_ADDR).unwrap();
        let re = decoded.encode().unwrap();
        assert_eq!(re, TV1_TESTNET_ADDR);
        assert_eq!(decoded.network, SilentPaymentNetwork::Testnet);
    }

    // ─── Positive: TV1 pinned encode ───────────────────────────────────────

    #[test]
    fn tv1_mainnet_encode_pinned() {
        let addr = SilentPaymentAddress::new(
            pk(TV1_SCAN_PK_BYTES),
            pk(TV1_SPEND_PK_BYTES),
            SilentPaymentNetwork::Mainnet,
        );
        assert_eq!(addr.encode().unwrap(), TV1_MAINNET_ADDR);
    }

    #[test]
    fn tv1_testnet_encode_pinned() {
        let addr = SilentPaymentAddress::new(
            pk(TV1_SCAN_PK_BYTES),
            pk(TV1_SPEND_PK_BYTES),
            SilentPaymentNetwork::Testnet,
        );
        assert_eq!(addr.encode().unwrap(), TV1_TESTNET_ADDR);
    }

    // ─── Positive: TV3 labeled pinned encode ───────────────────────────────

    #[test]
    fn tv3_mainnet_labeled_pinned() {
        let addr = SilentPaymentAddress::new(
            pk(TV1_SCAN_PK_BYTES),
            pk(TV3_B_M_BYTES),
            SilentPaymentNetwork::Mainnet,
        );
        assert_eq!(addr.encode().unwrap(), TV3_MAINNET_LABELED_ADDR);
    }

    #[test]
    fn tv3_testnet_labeled_pinned() {
        let addr = SilentPaymentAddress::new(
            pk(TV1_SCAN_PK_BYTES),
            pk(TV3_B_M_BYTES),
            SilentPaymentNetwork::Testnet,
        );
        assert_eq!(addr.encode().unwrap(), TV3_TESTNET_LABELED_ADDR);
    }

    // ─── Negative: wrong HRP ───────────────────────────────────────────────

    #[test]
    fn decode_xch_hrp_rejected() {
        // Standard Chia address from crates/chia-sdk-utils/src/bech32.rs:126 — bech32m, valid,
        // but HRP is "xch" not "spxch".
        let result = SilentPaymentAddress::decode(
            "xch1a0t57qn6uhe7tzjlxlhwy2qgmuxvvft8gnfzmg5detg0q9f3yc3s2apz0h",
        );
        match result {
            Err(SilentPaymentError::WrongHrp(hrp)) => assert_eq!(hrp, "xch"),
            other => panic!("expected WrongHrp(\"xch\"), got {other:?}"),
        }
    }

    // ─── Negative: invalid checksum ────────────────────────────────────────

    #[test]
    fn decode_invalid_checksum_rejected() {
        // Mutate the LAST character of TV1_MAINNET_ADDR. bech32m's last 6 chars
        // are checksum; changing the very last one definitely invalidates it.
        let mut s = TV1_MAINNET_ADDR.to_string();
        let last_char = s.chars().last().unwrap();
        // Pick a definitely-different bech32 alphabet character.
        let mutated = if last_char == 'q' { 'p' } else { 'q' };
        s.pop();
        s.push(mutated);
        let result = SilentPaymentAddress::decode(&s);
        assert!(
            matches!(result, Err(SilentPaymentError::Bech32(_))),
            "expected Bech32 error from checksum corruption, got {result:?}",
        );
    }

    // ─── Negative: bech32-not-bech32m ──────────────────────────────────────

    #[test]
    fn decode_bech32_not_bech32m_rejected() {
        // Bitcoin SegWit v0 address from crates/chia-sdk-utils/src/bech32.rs:139 — valid bech32
        // (the V0 variant) but NOT bech32m. The wrapper Bech32::decode returns
        // Bech32Error::InvalidFormat for any non-bech32m variant, which surfaces
        // through SilentPaymentError::Bech32(_).
        let result = SilentPaymentAddress::decode("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq");
        assert!(
            matches!(result, Err(SilentPaymentError::Bech32(_))),
            "expected Bech32 error for non-bech32m string, got {result:?}",
        );
    }

    // ─── Negative: short payload ───────────────────────────────────────────

    #[test]
    fn decode_short_payload_rejected() {
        // Build a valid bech32m string over a 32-byte payload (puzzle-hash size),
        // HRP "spxch". `decode` accepts the bech32m, then rejects on length.
        let short = Bech32::new(Bytes::new(vec![0u8; 32]), "spxch".to_string())
            .encode()
            .expect("Bech32 encode of 32-byte payload");
        match SilentPaymentAddress::decode(&short) {
            Err(SilentPaymentError::PayloadLength(n)) => assert_eq!(n, 32),
            other => panic!("expected PayloadLength(32), got {other:?}"),
        }
    }

    // ─── Negative: identity-element scan pubkey ────────────────────────────

    #[test]
    fn decode_identity_scan_pk_rejected() {
        // BLS12-381 compressed-infinity is `0xc0 || [0u8; 47]`. Whether
        // PublicKey::from_bytes accepts this (returning the identity point)
        // or rejects it is chia-bls-internal; either way, the address must
        // be rejected — CHIP §215 forbids identity-element halves.
        let mut payload = vec![0u8; 96];
        payload[0] = 0xc0; // BLS compressed-infinity flag
        payload[48..].copy_from_slice(&TV1_SPEND_PK_BYTES);
        let s = Bech32::new(Bytes::new(payload), "spxch".to_string())
            .encode()
            .expect("Bech32 encode of 96-byte payload");
        let result = SilentPaymentAddress::decode(&s);
        // Either IdentityPublicKey (if chia-bls decodes infinity successfully)
        // or InvalidPublicKey (if chia-bls rejects the encoding). Both satisfy
        // the CHIP §215 rejection requirement.
        assert!(
            matches!(
                result,
                Err(SilentPaymentError::IdentityPublicKey | SilentPaymentError::InvalidPublicKey)
            ),
            "expected IdentityPublicKey or InvalidPublicKey for identity scan_pk, got {result:?}",
        );
    }

    // ─── Negative: identity-element spend pubkey ───────────────────────────

    #[test]
    fn decode_identity_spend_pk_rejected() {
        let mut payload = vec![0u8; 96];
        payload[..48].copy_from_slice(&TV1_SCAN_PK_BYTES);
        payload[48] = 0xc0; // BLS compressed-infinity flag on the spend half
        let s = Bech32::new(Bytes::new(payload), "spxch".to_string())
            .encode()
            .expect("Bech32 encode of 96-byte payload");
        let result = SilentPaymentAddress::decode(&s);
        assert!(
            matches!(
                result,
                Err(SilentPaymentError::IdentityPublicKey | SilentPaymentError::InvalidPublicKey)
            ),
            "expected IdentityPublicKey or InvalidPublicKey for identity spend_pk, got {result:?}",
        );
    }
}
