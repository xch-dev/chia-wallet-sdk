use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Curried arguments for the MIPS index wrapper, which gives an inner puzzle a different hash
/// without changing its behavior.
///
/// The puzzle ignores the nonce and runs the inner puzzle with the solution as is. This lets the
/// same member appear more than once in a merkle tree, or be distinguished between vaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct IndexWrapperArgs<N, I> {
    /// An arbitrary value that only affects the puzzle hash.
    pub nonce: N,
    /// The inner puzzle, which is run with the solution.
    pub inner_puzzle: I,
}

impl<N, I> IndexWrapperArgs<N, I> {
    /// Creates new [`IndexWrapperArgs`] from the nonce and inner puzzle.
    pub fn new(nonce: N, inner_puzzle: I) -> Self {
        Self {
            nonce,
            inner_puzzle,
        }
    }
}

impl<N, I> Mod for IndexWrapperArgs<N, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INDEX_WRAPPER)
    }

    fn mod_hash() -> TreeHash {
        INDEX_WRAPPER_HASH
    }
}

/// The serialized CLVM bytecode of the MIPS index wrapper, `(a 5 7)`. See [`IndexWrapperArgs`].
pub const INDEX_WRAPPER: [u8; 7] = hex!("ff02ff05ff0780");

/// The tree hash of [`INDEX_WRAPPER`].
pub const INDEX_WRAPPER_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));
