//! Silent-payment bech32m address (HRP `spxch` mainnet / `tspxch` testnet)
//! and the network discriminant.

use bech32::{Variant, u5};
use chia_bls::PublicKey;

use crate::Bech32Error;

use super::SilentPaymentError;

/// The CHIP-0057 address version this implementation emits (v0).
pub const SP_ADDRESS_VERSION: u8 = 0;

/// The version reserved for backward-incompatible upgrades; decoding MUST fail.
pub const SP_ADDRESS_VERSION_BACKWARD_INCOMPATIBLE: u8 = 31;

/// Maximum silent-payment address length in characters. SP addresses exceed
/// bech32's original 90-character limit; CHIP-0057 (following BIP-352) allows
/// up to 1,023 characters.
pub const SP_ADDRESS_MAX_LENGTH: usize = 1023;

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

/// A CHIP-0057 silent-payment address: bech32m data part of a single 5-bit
/// version character (v0) followed by `serialize(B_scan) || serialize(B_spend)`
/// (96 bytes), under HRP `spxch` (mainnet) / `tspxch` (testnet).
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

    /// Encode as bech32m: HRP `||` `"1"` `||` base32(version `||` `scan_pk` `||` `spend_pk`) `||` checksum.
    ///
    /// The data part is a single 5-bit version character (v0) followed by the
    /// 96-byte payload `serialize(B_scan) || serialize(B_spend)` (CHIP-0057
    /// Address Versioning).
    pub fn encode(&self) -> Result<String, SilentPaymentError> {
        let mut payload = Vec::with_capacity(96);
        payload.extend_from_slice(&self.scan_pk.to_bytes());
        payload.extend_from_slice(&self.spend_pk.to_bytes());
        debug_assert_eq!(payload.len(), 96);

        let mut data = vec![u5::try_from_u8(SP_ADDRESS_VERSION).expect("SP_ADDRESS_VERSION < 32")];
        data.extend(
            bech32::convert_bits(&payload, 8, 5, true)
                .expect("8->5 with padding never fails")
                .into_iter()
                .map(u5::try_from_u8)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Bech32Error::from)?,
        );
        Ok(
            bech32::encode(self.network.hrp(), data, Variant::Bech32m)
                .map_err(Bech32Error::from)?,
        )
    }

    /// Decode a bech32m silent-payment address (CHIP-0057 Address Versioning).
    ///
    /// Version rules:
    ///   - v0: payload must be exactly 96 bytes.
    ///   - v1-30 (forward-compatible): payload must be at least 96 bytes; the
    ///     first 96 bytes are the keys and trailing data is ignored. Note that
    ///     re-encoding such an address emits v0 with only the keys.
    ///   - v31: reserved for backward-incompatible upgrades — always rejected.
    ///
    /// Rejects:
    ///   - length > 1,023 characters → `SilentPaymentError::AddressTooLong(N)`.
    ///   - HRP not in `{"spxch", "tspxch"}` → `SilentPaymentError::WrongHrp`.
    ///   - bech32 / bech32m parse failure → `SilentPaymentError::Bech32(_)`.
    ///   - non-bech32m variant (plain bech32) → `SilentPaymentError::Bech32(Bech32Error::InvalidFormat)`.
    ///   - version 31 → `SilentPaymentError::ReservedAddressVersion`.
    ///   - invalid payload length for the version → `SilentPaymentError::PayloadLength(N)`.
    ///   - either pubkey half is invalid bytes → `SilentPaymentError::InvalidPublicKey`.
    ///   - either pubkey half is the identity element → `SilentPaymentError::IdentityPublicKey`.
    pub fn decode(s: &str) -> Result<Self, SilentPaymentError> {
        if s.len() > SP_ADDRESS_MAX_LENGTH {
            return Err(SilentPaymentError::AddressTooLong(s.len()));
        }
        let (hrp, data, variant) = bech32::decode(s).map_err(Bech32Error::from)?;
        if variant != Variant::Bech32m {
            return Err(Bech32Error::InvalidFormat.into());
        }
        let network = SilentPaymentNetwork::from_hrp(&hrp)?;
        let Some((version, rest)) = data.split_first() else {
            return Err(SilentPaymentError::PayloadLength(0));
        };
        let version = version.to_u8();
        if version == SP_ADDRESS_VERSION_BACKWARD_INCOMPATIBLE {
            return Err(SilentPaymentError::ReservedAddressVersion);
        }
        let payload = bech32::convert_bits(rest, 5, 8, false).map_err(Bech32Error::from)?;
        let length_ok = if version == SP_ADDRESS_VERSION {
            payload.len() == 96
        } else {
            // Forward-compatible versions may append data after the keys.
            payload.len() >= 96
        };
        if !length_ok {
            return Err(SilentPaymentError::PayloadLength(payload.len()));
        }
        let scan_bytes: [u8; 48] = payload[..48].try_into().expect("payload >= 96");
        let spend_bytes: [u8; 48] = payload[48..96].try_into().expect("payload >= 96");
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

    const TV1_MAINNET_ADDR: &str = "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsmqrz7f";
    const TV1_TESTNET_ADDR: &str = "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uws0zz572";

    /// The pre-versioning (legacy) encoding of TV1 mainnet — no version
    /// character. MUST be rejected by the versioned decoder.
    const TV1_LEGACY_MAINNET_ADDR: &str = "spxch15p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsfn2u2q";

    // ─── TV3 (CHIP-0057 test vector 3 — labeled, m = 1) ────────────────────
    // B_m = B_spend + label_pk; recipient's scan_pk is unchanged from TV1.

    const TV3_B_M_BYTES: [u8; 48] = hex!(
        "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e962"
        "77d1f02fbbe01cee412dd2ce1b7414c2"
    );

    const TV3_MAINNET_LABELED_ADDR: &str = "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cg8n8kqj";
    const TV3_TESTNET_LABELED_ADDR: &str = "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cgn3xqq3";

    // Helper: parse a 48-byte hex array into a chia_bls::PublicKey.
    fn pk(bytes: [u8; 48]) -> PublicKey {
        PublicKey::from_bytes(&bytes).expect("test vector pubkey")
    }

    // Helper: build an SP address with an arbitrary version character and
    // payload, bypassing `encode`'s v0/96-byte construction.
    fn encode_with_version(version: u8, payload: &[u8], hrp: &str) -> String {
        let mut data = vec![u5::try_from_u8(version).expect("version < 32")];
        data.extend(
            bech32::convert_bits(payload, 8, 5, true)
                .expect("8->5 with padding never fails")
                .into_iter()
                .map(|b| u5::try_from_u8(b).expect("5-bit value")),
        );
        bech32::encode(hrp, data, Variant::Bech32m).expect("bech32m encode")
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
        // Build a valid v0 bech32m string over a 32-byte payload (puzzle-hash
        // size), HRP "spxch". `decode` accepts the bech32m, then rejects on length.
        let short = encode_with_version(0, &[0u8; 32], "spxch");
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
        let s = encode_with_version(0, &payload, "spxch");
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
        let s = encode_with_version(0, &payload, "spxch");
        let result = SilentPaymentAddress::decode(&s);
        assert!(
            matches!(
                result,
                Err(SilentPaymentError::IdentityPublicKey | SilentPaymentError::InvalidPublicKey)
            ),
            "expected IdentityPublicKey or InvalidPublicKey for identity spend_pk, got {result:?}",
        );
    }

    // ─── CHIP-0057 Address Versioning ──────────────────────────────────────

    #[test]
    fn v0_version_char_and_length() {
        // The first data character is the version: v0 encodes as 'q'.
        // A v0 address is 167 chars (spxch) / 168 chars (tspxch).
        assert_eq!(TV1_MAINNET_ADDR.as_bytes()["spxch1".len()], b'q');
        assert_eq!(TV1_MAINNET_ADDR.len(), 167);
        assert_eq!(TV1_TESTNET_ADDR.len(), 168);
    }

    #[test]
    fn decode_legacy_versionless_rejected() {
        // The pre-versioning encoding must fail loudly, never mis-parse: the
        // leading key bits misread as a version, leaving a 95-byte payload.
        let result = SilentPaymentAddress::decode(TV1_LEGACY_MAINNET_ADDR);
        assert!(
            result.is_err(),
            "legacy versionless address must be rejected, got {result:?}",
        );
    }

    #[test]
    fn decode_forward_compatible_unknown_versions() {
        // v1-30 with trailing data: the first 96 payload bytes are the keys,
        // the rest is ignored. Re-encoding normalizes to v0.
        let mut payload = Vec::with_capacity(106);
        payload.extend_from_slice(&TV1_SCAN_PK_BYTES);
        payload.extend_from_slice(&TV1_SPEND_PK_BYTES);
        payload.extend_from_slice(&[0x42; 10]);

        for version in [1, 30] {
            let s = encode_with_version(version, &payload, "spxch");
            let decoded = SilentPaymentAddress::decode(&s).unwrap();
            assert_eq!(decoded.scan_pk, pk(TV1_SCAN_PK_BYTES));
            assert_eq!(decoded.spend_pk, pk(TV1_SPEND_PK_BYTES));
            assert_eq!(decoded.network, SilentPaymentNetwork::Mainnet);
            assert_eq!(decoded.encode().unwrap(), TV1_MAINNET_ADDR);
        }
    }

    #[test]
    fn decode_unknown_version_short_payload_rejected() {
        // A v1-30 address must still carry at least the 96 key bytes.
        let s = encode_with_version(1, &[0u8; 95], "spxch");
        match SilentPaymentAddress::decode(&s) {
            Err(SilentPaymentError::PayloadLength(n)) => assert_eq!(n, 95),
            other => panic!("expected PayloadLength(95), got {other:?}"),
        }
    }

    #[test]
    fn decode_version_31_rejected() {
        // v31 is the backward-incompatible sentinel: reject even a well-formed
        // 96-byte payload.
        let mut payload = Vec::with_capacity(96);
        payload.extend_from_slice(&TV1_SCAN_PK_BYTES);
        payload.extend_from_slice(&TV1_SPEND_PK_BYTES);
        let s = encode_with_version(31, &payload, "spxch");
        match SilentPaymentAddress::decode(&s) {
            Err(SilentPaymentError::ReservedAddressVersion) => {}
            other => panic!("expected ReservedAddressVersion, got {other:?}"),
        }
    }

    #[test]
    fn decode_over_max_length_rejected() {
        // 96 key bytes + 600 trailing bytes exceeds the 1,023-char allowance.
        let mut payload = vec![0u8; 696];
        payload[..48].copy_from_slice(&TV1_SCAN_PK_BYTES);
        payload[48..96].copy_from_slice(&TV1_SPEND_PK_BYTES);
        let s = encode_with_version(1, &payload, "spxch");
        assert!(s.len() > SP_ADDRESS_MAX_LENGTH);
        match SilentPaymentAddress::decode(&s) {
            Err(SilentPaymentError::AddressTooLong(n)) => assert_eq!(n, s.len()),
            other => panic!("expected AddressTooLong, got {other:?}"),
        }
    }
}
