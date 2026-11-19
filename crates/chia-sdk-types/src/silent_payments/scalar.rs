//! [`ScalarField`]: the protocol's scalars, reduced mod the BLS12-381 subgroup order as
//! unsigned integers. There is no `From<[u8; 32]>`, so the reduction is always chosen explicitly.

use std::fmt;

use num_bigint::BigUint;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// The BLS12-381 subgroup order `r` as big-endian bytes.
pub const GROUP_ORDER: [u8; 32] = [
    0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48, 0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
    0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
];

/// A scalar as 32 big-endian bytes: below `r` from [`Self::from_bytes_unsigned`] and after any
/// arithmetic, unchecked from [`Self::from_bytes_raw`].
///
/// An output tweak or a label scalar links a coin or an address to the wallet, so `Debug` is
/// redacted, the bytes are zeroized on drop and the type is not `Copy`. The arithmetic is not
/// constant-time, so secret keys are added with `chia_bls::SecretKey`, not with this type.
#[derive(Clone, PartialEq, Eq)]
pub struct ScalarField([u8; 32]);

impl fmt::Debug for ScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScalarField(<redacted>)")
    }
}

impl Zeroize for ScalarField {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for ScalarField {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for ScalarField {}

impl ScalarField {
    /// Reduces 32 big-endian bytes mod `r` as an unsigned integer, as CHIP-0057 requires for the
    /// input hash, output tweaks and label scalars. The signed reduction of the standard puzzle's
    /// synthetic offset differs when the high bit is set and would give undetectable payments.
    #[must_use]
    pub fn from_bytes_unsigned(bytes: [u8; 32]) -> Self {
        let n = BigUint::from_bytes_be(&bytes);
        let r = BigUint::from_bytes_be(&GROUP_ORDER);
        let result = n % &r;
        Self(biguint_to_be_bytes_32(&result))
    }

    /// Wraps bytes without reducing them; the caller ensures the value is below `r`.
    #[must_use]
    pub fn from_bytes_raw(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// `(self + other) mod r`.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let a = BigUint::from_bytes_be(&self.0);
        let b = BigUint::from_bytes_be(&other.0);
        let r = BigUint::from_bytes_be(&GROUP_ORDER);
        let result = (a + b) % &r;
        Self(biguint_to_be_bytes_32(&result))
    }

    /// The 32 big-endian bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// A copy of the 32 big-endian bytes, which is not zeroized on drop.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Whether the scalar is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

/// Left-pads a value below `2^256` to 32 big-endian bytes.
fn biguint_to_be_bytes_32(value: &BigUint) -> [u8; 32] {
    let be = value.to_bytes_be();
    let mut out = [0u8; 32];
    if !be.is_empty() {
        out[32 - be.len()..].copy_from_slice(&be);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_unsigned_max_input_reduces_unsigned_not_identity() {
        // The all-ones input `[0xff; 32]` (i.e. `2^256 - 1`) reduces unsigned to
        // the pinned constant below. For BLS12-381's r the quotient is
        // `floor((2^256 - 1) / r) = 2`, so the remainder is `(2^256 - 1) - 2r`.
        // This is NOT `r - 1`. The point of the test is that unsigned reduction
        // fired: the reduced value differs from the raw input.
        let s = ScalarField::from_bytes_unsigned([0xff; 32]);
        let expected: [u8; 32] = [
            0x18, 0x24, 0xb1, 0x59, 0xac, 0xc5, 0x05, 0x6f, 0x99, 0x8c, 0x4f, 0xef, 0xec, 0xbc,
            0x4f, 0xf5, 0x58, 0x84, 0xb7, 0xfa, 0x00, 0x03, 0x48, 0x02, 0x00, 0x00, 0x00, 0x01,
            0xff, 0xff, 0xff, 0xfd,
        ];
        assert_eq!(s.to_bytes(), expected);
        assert_ne!(s.to_bytes(), [0xff; 32]);
    }

    #[test]
    fn from_bytes_unsigned_identity() {
        // A value strictly less than `r` passes through `from_bytes_unsigned`
        // unchanged — no reduction is needed.
        let mut bytes = [0u8; 32];
        bytes[31] = 42;
        let s = ScalarField::from_bytes_unsigned(bytes);
        assert_eq!(s.to_bytes(), bytes);
    }

    #[test]
    fn add_wraps_at_r() {
        // (r - 1) + 1 = r ≡ 0 (mod r). Construct r-1 from GROUP_ORDER by zeroing
        // the last byte (GROUP_ORDER ends in 0x01).
        let mut r_minus_1 = GROUP_ORDER;
        r_minus_1[31] = 0x00;
        let a = ScalarField::from_bytes_raw(r_minus_1);
        let mut one = [0u8; 32];
        one[31] = 1;
        let b = ScalarField::from_bytes_raw(one);
        let c = a.add(&b);
        assert!(c.is_zero());
    }

    #[test]
    fn from_bytes_raw_does_not_reduce() {
        // [0xff; 32] is intentionally >= r; from_bytes_raw is the unchecked escape
        // hatch and must NOT silently reduce.
        let s = ScalarField::from_bytes_raw([0xff; 32]);
        assert_eq!(s.to_bytes(), [0xff; 32]);
    }

    /// `Debug` must not reveal the value, in either the plain or the
    /// alternate (pretty) form.
    #[test]
    fn debug_is_redacted() {
        let s = ScalarField::from_bytes_raw([0xab; 32]);
        for rendered in [format!("{s:?}"), format!("{s:#?}")] {
            assert_eq!(rendered, "ScalarField(<redacted>)");
            assert!(!rendered.contains("ab"));
            assert!(!rendered.contains("171"));
        }
    }

    /// The type zeroizes on drop: `Drop` runs `zeroize`, which overwrites the
    /// bytes. (Memory cannot be inspected after a drop without `unsafe`, which
    /// this workspace denies, so the two halves are checked separately.)
    #[test]
    fn zeroizes_on_drop() {
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}
        assert_zeroize_on_drop::<ScalarField>();

        let mut s = ScalarField::from_bytes_raw([0xab; 32]);
        s.zeroize();
        assert_eq!(s.to_bytes(), [0u8; 32]);
    }
}
