//! The tagged hash and its tags (CHIP-0057 "Tagged Hash").

use chia_sha2::Sha256;

/// The tag of the input hash.
pub const CHIA_SP_INPUTS: &str = "Chia_SP/Inputs";

/// The tag of an output tweak.
pub const CHIA_SP_SHARED_SECRET: &str = "Chia_SP/SharedSecret";

/// The tag of a label scalar.
pub const CHIA_SP_LABEL: &str = "Chia_SP/Label";

/// `SHA256(SHA256(tag) || SHA256(tag) || data)`, the BIP-340 tagged hash.
#[must_use]
pub fn tagged_hash(tag: &str, data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(tag.as_bytes());
    let tag_hash: [u8; 32] = h.finalize();

    let mut h = Sha256::new();
    h.update(tag_hash);
    h.update(tag_hash);
    h.update(data);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    /// Single-shot SHA-256 over a byte slice using `chia-sha2`.
    fn sha256(bytes: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(bytes);
        h.finalize()
    }

    #[test]
    fn tagged_hash_matches_bip340_challenge_vector() {
        // BIP-340 published value:
        //   SHA256(SHA256("BIP0340/challenge") || SHA256("BIP0340/challenge") || "")
        //   = c216d352f5818b7b4beacd4ae0a26fe888080823d2a598856661bcd54f1b3713
        // This cross-implementation vector proves the construction itself is
        // BIP-340-compatible — independent of any Chia-specific tags.
        const EXPECTED: [u8; 32] =
            hex!("c216d352f5818b7b4beacd4ae0a26fe888080823d2a598856661bcd54f1b3713");
        assert_eq!(tagged_hash("BIP0340/challenge", b""), EXPECTED);
    }

    #[test]
    fn tag_inputs_hash_pinned() {
        // SHA256(b"Chia_SP/Inputs"). Cross-verified via Python `hashlib.sha256`
        // and `sha256sum` CLI — both produced identical bytes. Any rename of
        // the `CHIA_SP_INPUTS` value (e.g. to "ChiaSP/Inputs" or
        // "Chia_SP/Input") fails this test before any protocol code runs.
        const EXPECTED: [u8; 32] =
            hex!("d44a6db8619ff9fb78e318cb49b6bae45b338c6b085b98c42e3e43ffda3378cf");
        assert_eq!(sha256(CHIA_SP_INPUTS.as_bytes()), EXPECTED);
    }

    #[test]
    fn tag_shared_secret_hash_pinned() {
        // SHA256(b"Chia_SP/SharedSecret"). Cross-verified via Python and
        // `sha256sum` CLI — both produced identical bytes.
        const EXPECTED: [u8; 32] =
            hex!("e7b8a5241d3764f92efc3f2f0b634279c7feddb3101021efd897a8dd63b9ca9f");
        assert_eq!(sha256(CHIA_SP_SHARED_SECRET.as_bytes()), EXPECTED);
    }

    #[test]
    fn tag_label_hash_pinned() {
        // SHA256(b"Chia_SP/Label"). Cross-verified via Python and `sha256sum`
        // CLI — both produced identical bytes.
        const EXPECTED: [u8; 32] =
            hex!("c63c8bd2123be129023b8f24c0249bacbd83da15e2b883ec5a03aa62b0f94554");
        assert_eq!(sha256(CHIA_SP_LABEL.as_bytes()), EXPECTED);
    }

    #[test]
    fn different_tags_different_outputs() {
        assert_ne!(tagged_hash("tag1", b"data"), tagged_hash("tag2", b"data"));
    }
}
