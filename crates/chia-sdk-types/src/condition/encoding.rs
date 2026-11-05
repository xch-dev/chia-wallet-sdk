use chia_puzzle_types::Memos;
use clvm_traits::{ClvmDecoder, FromClvm, FromClvmError, encode_number};
use clvm_utils::{TreeHash, tree_hash_atom};
use clvmr::NodePtr;

/// A value that represents nil, used for the arguments that follow a condition's known arguments.
pub trait Nil {
    fn nil() -> Self;
}

impl Nil for NodePtr {
    fn nil() -> Self {
        NodePtr::NIL
    }
}

impl Nil for TreeHash {
    fn nil() -> Self {
        tree_hash_atom(&[])
    }
}

impl Nil for () {
    fn nil() -> Self {}
}

/// A field that may be absent from the end of a condition's arguments. This is different from being nil,
/// since a nil value is still present, and the condition must serialize back to the same value.
pub trait OptionalElement {
    type Item;

    fn from_item(item: Option<Self::Item>) -> Self;
    fn item(&self) -> Option<&Self::Item>;
}

impl<T> OptionalElement for Option<T> {
    type Item = T;

    fn from_item(item: Option<T>) -> Self {
        item
    }

    fn item(&self) -> Option<&T> {
        self.as_ref()
    }
}

impl<T> OptionalElement for Memos<T> {
    type Item = T;

    fn from_item(item: Option<T>) -> Self {
        match item {
            Some(item) => Self::Some(item),
            None => Self::None,
        }
    }

    fn item(&self) -> Option<&T> {
        match self {
            Self::Some(item) => Some(item),
            Self::None => None,
        }
    }
}

/// An integer that can be checked for a canonical encoding.
pub trait CanonicalInt {
    fn canonical_bytes(&self) -> Vec<u8>;
}

macro_rules! impl_canonical_int {
    ( $( $ty:ty ),* ) => {
        $(
            impl CanonicalInt for $ty {
                #[allow(unused_comparisons)]
                fn canonical_bytes(&self) -> Vec<u8> {
                    encode_number(&self.to_be_bytes(), *self < 0)
                }
            }
        )*
    };
}

impl_canonical_int!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);

/// Decodes an integer, and makes sure it's canonically encoded. Consensus rejects redundant leading
/// bytes, and accepting them would mean the value doesn't serialize back to the same atom.
pub fn decode_canonical_int<D, I>(decoder: &D, node: D::Node) -> Result<I, FromClvmError>
where
    D: ClvmDecoder,
    I: FromClvm<D> + CanonicalInt,
{
    let atom = decoder.decode_atom(&node)?.as_ref().to_vec();
    let value = I::from_clvm(decoder, node)?;

    if value.canonical_bytes() != atom {
        return Err(FromClvmError::Custom(
            "integer is not canonically encoded".to_string(),
        ));
    }

    Ok(value)
}

/// Checks that a node is exactly the given atom.
pub fn expect_atom<D>(decoder: &D, node: &D::Node, expected: &[u8]) -> Result<(), FromClvmError>
where
    D: ClvmDecoder,
{
    if decoder.decode_atom(node)?.as_ref() != expected {
        return Err(FromClvmError::Custom(
            "constant has an incorrect value".to_string(),
        ));
    }

    Ok(())
}
