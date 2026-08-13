//! `ScalarField` newtype: big-endian 32-byte scalar with unsigned mod-r arithmetic
//! over the BLS12-381 subgroup order.
//!
//! All CHIP-0057 protocol scalar values (input hash, output tweak, label scalar) use
//! UNSIGNED interpretation mod `GROUP_ORDER`. This is intentionally distinct from
//! the signed mod-r reducer used by the standard-puzzle synthetic-key offset (in
//! `chia_puzzle_types::derive_synthetic`), which interprets the input bytes as a
//! SIGNED two's-complement integer. The two routes silently disagree on any input
//! whose high bit is set, so this module keeps them apart at the type level: the
//! only way to obtain a `ScalarField` from arbitrary bytes is to choose between
//! [`ScalarField::from_bytes_unsigned`] (which performs unsigned mod-r reduction)
//! and [`ScalarField::from_bytes_raw`] (which performs no reduction at all).
//!
//! There is deliberately no `From<[u8; 32]>` impl. A bare conversion would erase
//! the unsigned-vs-signed choice and reintroduce the protocol-correctness hazard.

use num_bigint::BigUint;

/// BLS12-381 subgroup order `r` as big-endian bytes.
///
/// Matches `chia_puzzle_types::derive_synthetic::GROUP_ORDER_BYTES` byte-for-byte;
/// duplicated here so the `chip-0057` module is self-contained and the unsigned
/// reduction path has no dependency on the standard-puzzle module.
pub const GROUP_ORDER: [u8; 32] = [
    0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48, 0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
    0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
];

/// A scalar field element: 32 big-endian bytes representing a value in `[0, r)`
/// when constructed via [`Self::from_bytes_unsigned`], or an unchecked 32-byte
/// value when constructed via [`Self::from_bytes_raw`].
///
/// Arithmetic operations always reduce mod `r` before returning; the only way to
/// observe a non-reduced value is to construct one via `from_bytes_raw` and read
/// it back via [`Self::as_bytes`] / [`Self::to_bytes`] without performing arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarField([u8; 32]);

impl ScalarField {
    /// Reduce a 32-byte big-endian value mod `r` using UNSIGNED interpretation.
    ///
    /// This is the correct operation for CHIP-0057 protocol scalars (input hash,
    /// output tweak, label scalar). Do NOT replace this with the signed mod-r
    /// reducer used by the standard-puzzle synthetic-key offset — the two routes
    /// disagree on inputs whose high bit is set, and a silent crossover would
    /// produce undetectable silent payments.
    #[must_use]
    pub fn from_bytes_unsigned(bytes: [u8; 32]) -> Self {
        let n = BigUint::from_bytes_be(&bytes);
        let r = BigUint::from_bytes_be(&GROUP_ORDER);
        let result = n % &r;
        Self(biguint_to_be_bytes_32(&result))
    }

    /// Wrap raw bytes without any reduction.
    ///
    /// The caller is responsible for ensuring `bytes` represents a value in
    /// `[0, r)` if that invariant matters downstream. This is the unchecked
    /// escape hatch used for values known to be in range (e.g., secret-key
    /// bytes drawn from a domain that guarantees `< r`).
    #[must_use]
    pub fn from_bytes_raw(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Compute `(self + other) mod r`.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let a = BigUint::from_bytes_be(&self.0);
        let b = BigUint::from_bytes_be(&other.0);
        let r = BigUint::from_bytes_be(&GROUP_ORDER);
        let result = (a + b) % &r;
        Self(biguint_to_be_bytes_32(&result))
    }

    /// Compute `(self * other) mod r`.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let a = BigUint::from_bytes_be(&self.0);
        let b = BigUint::from_bytes_be(&other.0);
        let r = BigUint::from_bytes_be(&GROUP_ORDER);
        let result = (a * b) % &r;
        Self(biguint_to_be_bytes_32(&result))
    }

    /// Return a reference to the inner 32 big-endian bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Return a copy of the inner 32 big-endian bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Return `true` if the scalar's byte representation is all zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

/// Left-pad a `BigUint` into a 32-byte big-endian array, dropping leading zeros
/// produced by `to_bytes_be`. Panics only if the value exceeds 32 bytes — which
/// cannot happen for any output of `% GROUP_ORDER` since `r < 2^256`.
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
    fn mul_mod_r() {
        // 2 * 3 = 6 (no reduction needed; pins the multiplicative semantics)
        let mut a_bytes = [0u8; 32];
        a_bytes[31] = 2;
        let mut b_bytes = [0u8; 32];
        b_bytes[31] = 3;
        let a = ScalarField::from_bytes_unsigned(a_bytes);
        let b = ScalarField::from_bytes_unsigned(b_bytes);
        let c = a.mul(&b);
        let mut expected = [0u8; 32];
        expected[31] = 6;
        assert_eq!(c.to_bytes(), expected);
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
}
