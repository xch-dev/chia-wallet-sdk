//! Strict integer decoding for condition arguments.
//!
//! The [`FromClvm`](clvm_traits::FromClvm) implementations for primitive integers in `clvm-traits`
//! are lenient. They ignore the sign for unsigned types (so `-113` decodes as `143`u64), and they
//! truncate oversized atoms whose first byte is padding (so `0x00 01 <8 bytes>` decodes as the low
//! 8 bytes). Consensus rejects or reinterprets both cases, so condition parsing uses these helpers
//! instead, which mirror `sanitize_uint` and `parse_opcode` from `chia-consensus`.

use clvm_traits::{ClvmDecoder, FromClvmError};

/// An integer that can be decoded from its canonical CLVM atom representation.
pub(crate) trait CanonicalInt: Sized {
    fn from_canonical_atom(atom: &[u8]) -> Option<Self>;
}

/// Decodes an atom as an unsigned integer of at most `max_size` bytes, following the same rules
/// as `sanitize_uint` in `chia-consensus`. Negative values, redundant leading zeros, and values
/// that don't fit are all rejected.
fn decode_unsigned(atom: &[u8], max_size: usize) -> Option<u64> {
    if atom.is_empty() {
        return Some(0);
    }

    // If the top bit is set, it's a negative number.
    if atom[0] & 0x80 != 0 {
        return None;
    }

    // A leading zero is only allowed if it prevents the value from being interpreted as negative.
    if atom == [0] || (atom.len() > 1 && atom[0] == 0 && atom[1] & 0x80 == 0) {
        return None;
    }

    let digits = if atom[0] == 0 { &atom[1..] } else { atom };

    if digits.len() > max_size {
        return None;
    }

    Some(
        digits
            .iter()
            .fold(0, |value, &byte| (value << 8) | u64::from(byte)),
    )
}

/// Decodes an atom as a signed integer of at most `max_size` bytes, rejecting any representation
/// other than the minimal two's complement encoding.
fn decode_signed(atom: &[u8], max_size: usize) -> Option<i64> {
    if atom.is_empty() {
        return Some(0);
    }

    if atom.len() > max_size {
        return None;
    }

    // A leading 0x00 or 0xFF byte is redundant if the next byte already has the same sign bit.
    if atom.len() > 1
        && ((atom[0] == 0x00 && atom[1] & 0x80 == 0) || (atom[0] == 0xFF && atom[1] & 0x80 != 0))
    {
        return None;
    }

    // A lone 0x00 byte is not the canonical representation of zero.
    if atom == [0] {
        return None;
    }

    let initial = if atom[0] & 0x80 != 0 { -1 } else { 0 };

    Some(
        atom.iter()
            .fold(initial, |value, &byte| (value << 8) | i64::from(byte)),
    )
}

macro_rules! impl_canonical_int {
    ( $decode:ident, $wide:ty => $( $ty:ty ),* ) => {
        $( impl CanonicalInt for $ty {
            fn from_canonical_atom(atom: &[u8]) -> Option<Self> {
                let value: $wide = $decode(atom, std::mem::size_of::<$ty>())?;
                Self::try_from(value).ok()
            }
        } )*
    };
}

impl_canonical_int!(decode_unsigned, u64 => u8, u16, u32, u64);
impl_canonical_int!(decode_signed, i64 => i8, i16, i32, i64);

/// Decodes a condition argument as a canonically encoded integer.
pub(crate) fn decode_canonical_int<D, T>(decoder: &D, node: &D::Node) -> Result<T, FromClvmError>
where
    D: ClvmDecoder,
    T: CanonicalInt,
{
    let atom = decoder.decode_atom(node)?;

    T::from_canonical_atom(atom.as_ref()).ok_or_else(|| {
        FromClvmError::Custom(format!(
            "expected a canonical {}, but found {:02x?}",
            std::any::type_name::<T>(),
            atom.as_ref()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use clvmr::Allocator;
    use rstest::rstest;

    fn decode<T: CanonicalInt>(atom: &[u8]) -> Option<T> {
        let mut allocator = Allocator::new();
        let node = allocator.new_atom(atom).unwrap();
        decode_canonical_int(&allocator, &node).ok()
    }

    #[rstest]
    #[case(&[], Some(0))]
    #[case(&[0x01], Some(1))]
    #[case(&[0x7F], Some(0x7F))]
    #[case(&[0x00, 0x80], Some(0x80))]
    #[case(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], Some(u64::MAX))]
    #[case(&[0x00], None)]
    #[case(&[0x00, 0x01], None)]
    #[case(&[0x8F], None)]
    #[case(&[0xFF], None)]
    #[case(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05], None)]
    #[case(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], None)]
    fn test_u64(#[case] atom: &[u8], #[case] expected: Option<u64>) {
        assert_eq!(decode::<u64>(atom), expected);
    }

    #[rstest]
    #[case(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF], Some(u32::MAX))]
    #[case(&[0x01, 0x00, 0x00, 0x00, 0x00], None)]
    #[case(&[0xFF], None)]
    fn test_u32(#[case] atom: &[u8], #[case] expected: Option<u32>) {
        assert_eq!(decode::<u32>(atom), expected);
    }

    #[rstest]
    #[case(&[0x3F], Some(63))]
    #[case(&[0x00, 0xFF], Some(255))]
    #[case(&[0x01, 0x00], None)]
    fn test_u8(#[case] atom: &[u8], #[case] expected: Option<u8>) {
        assert_eq!(decode::<u8>(atom), expected);
    }

    #[rstest]
    #[case(&[], Some(0))]
    #[case(&[0x33], Some(51))]
    #[case(&[0x8F], Some(-113))]
    #[case(&[0xFF], Some(-1))]
    #[case(&[0x80], Some(-128))]
    #[case(&[0x00], None)]
    #[case(&[0x00, 0x33], None)]
    #[case(&[0xFF, 0x8F], None)]
    #[case(&[0x00, 0x80], None)]
    fn test_i8(#[case] atom: &[u8], #[case] expected: Option<i8>) {
        assert_eq!(decode::<i8>(atom), expected);
    }
}
