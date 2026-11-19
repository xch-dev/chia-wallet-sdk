use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError};

/// A constant in a condition, such as its opcode, which only matches its canonical encoding.
///
/// Integers can be decoded from atoms with redundant leading bytes, so `0x0033` would otherwise
/// match the `CREATE_COIN` opcode `51`. But consensus and puzzles compare these values by their
/// exact bytes (like CLVM's `=` operator), so to them `(0x0033 ...)` is an unknown condition. The
/// `conditions!` macro wraps every constant field in this type so that parsing agrees with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canonical<T>(pub T);

impl<D: ClvmDecoder, T: FromClvm<D>> FromClvm<D> for Canonical<T> {
    fn from_clvm(decoder: &D, node: D::Node) -> Result<Self, FromClvmError> {
        if let Ok(atom) = decoder.decode_atom(&node)
            && !is_canonical(atom.as_ref())
        {
            return Err(FromClvmError::Custom(
                "constant has a non-canonical encoding".to_string(),
            ));
        }

        T::from_clvm(decoder, node).map(Self)
    }
}

impl<E: ClvmEncoder, T: ToClvm<E>> ToClvm<E> for Canonical<T> {
    fn to_clvm(&self, encoder: &mut E) -> Result<E::Node, ToClvmError> {
        self.0.to_clvm(encoder)
    }
}

/// Whether an integer atom has no redundant leading byte. Zero is only canonical as nil.
fn is_canonical(atom: &[u8]) -> bool {
    match atom {
        [0x00] => false,
        [0x00, next, ..] => next & 0x80 != 0,
        [0xff, next, ..] => next & 0x80 == 0,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use clvmr::{Allocator, NodePtr};
    use rstest::rstest;

    use crate::Condition;

    use super::*;

    #[rstest]
    #[case(&[], true)]
    #[case(&[0x00], false)]
    #[case(&[0x33], true)]
    #[case(&[0x00, 0x33], false)]
    #[case(&[0x00, 0x80], true)]
    #[case(&[0x8f], true)]
    #[case(&[0xff, 0x8f], false)]
    #[case(&[0xff, 0x7f], true)]
    fn test_canonical_constant(#[case] atom: &[u8], #[case] canonical: bool) {
        let mut allocator = Allocator::new();
        let ptr = allocator.new_atom(atom).unwrap();

        assert_eq!(
            Canonical::<i32>::from_clvm(&allocator, ptr).is_ok(),
            canonical
        );
    }

    /// Conditions are only recognized by their exact opcode and magic amount, like consensus and the puzzles do.
    /// Each atom is written in hex, with `()` for nil and `ph` for a puzzle hash.
    #[rstest]
    #[case("33 ph 01", Condition::is_create_coin)]
    #[case("0033 ph 01", is_other)]
    #[case("33 () 8f", Condition::is_melt_singleton)]
    #[case("33 () ff8f", is_other)]
    #[case("33 () 8f () ()", Condition::is_run_cat_tail)]
    #[case("0033 () 8f () ()", is_other)]
    fn test_canonical_condition(#[case] atoms: &str, #[case] expected: fn(&Condition) -> bool) {
        let mut allocator = Allocator::new();

        let atoms: Vec<NodePtr> = atoms
            .split_whitespace()
            .map(|atom| {
                let bytes = match atom {
                    "()" => Vec::new(),
                    "ph" => vec![1; 32],
                    hex => hex::decode(hex).unwrap(),
                };
                allocator.new_atom(&bytes).unwrap()
            })
            .collect();
        let ptr = atoms.to_clvm(&mut allocator).unwrap();

        let condition = Condition::<NodePtr>::from_clvm(&allocator, ptr).unwrap();

        assert!(expected(&condition), "{condition:?}");
    }

    fn is_other(condition: &Condition) -> bool {
        matches!(condition, Condition::Other(_))
    }
}
