use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{ADD_DPUZ_WRAPPER_HASH, ENFORCE_DPUZ_WRAPPERS, ENFORCE_DPUZ_WRAPPERS_HASH};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::Mod;

/// Curried arguments for the MIPS `enforce_dpuz_wrappers` restriction, a delegated puzzle
/// validator that requires the delegated puzzle to be wrapped in a fixed list of wrappers.
///
/// The solution reveals the hash of the innermost delegated puzzle. The restriction wraps that
/// hash in an [`AddDelegatedPuzzleWrapper`](crate::puzzles::AddDelegatedPuzzleWrapper) for each
/// wrapper, with the first wrapper outermost, and asserts that the result is the delegated puzzle
/// hash being validated.
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Mod, puzzles::{EnforceDelegatedPuzzleWrappers, PreventConditionOpcode}};
/// // Require every delegated puzzle to be wrapped so that it can't create coin announcements.
/// let wrapper = PreventConditionOpcode::new(60).curry_tree_hash();
/// let restriction = EnforceDelegatedPuzzleWrappers::new(&[wrapper]);
/// assert_eq!(restriction.quoted_wrapper_stack.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EnforceDelegatedPuzzleWrappers {
    /// The tree hash of the quoted `add_dpuz_wrapper` mod, `(q . ADD_DPUZ_WRAPPER)`.
    pub quoted_add_wrapper_mod_hash: Bytes32,
    /// The tree hash of each quoted wrapper, `(q . wrapper)`, with the outermost wrapper first.
    pub quoted_wrapper_stack: Vec<Bytes32>,
}

impl EnforceDelegatedPuzzleWrappers {
    /// Creates new [`EnforceDelegatedPuzzleWrappers`] from the tree hashes of the wrappers, with
    /// the outermost wrapper first. The hashes are quoted automatically.
    pub fn new(wrapper_stack: &[TreeHash]) -> Self {
        Self {
            quoted_add_wrapper_mod_hash: clvm_quote!(TreeHash::new(ADD_DPUZ_WRAPPER_HASH))
                .tree_hash()
                .into(),
            quoted_wrapper_stack: wrapper_stack
                .iter()
                .map(|wrapper| clvm_quote!(wrapper).tree_hash().into())
                .collect(),
        }
    }
}

impl Mod for EnforceDelegatedPuzzleWrappers {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ENFORCE_DPUZ_WRAPPERS)
    }

    fn mod_hash() -> TreeHash {
        ENFORCE_DPUZ_WRAPPERS_HASH.into()
    }
}

/// The solution for the MIPS `enforce_dpuz_wrappers` restriction. See
/// [`EnforceDelegatedPuzzleWrappers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct EnforceDelegatedPuzzleWrappersSolution {
    /// The tree hash of the innermost delegated puzzle, before any wrappers are added.
    pub inner_delegated_puzzle_hash: Bytes32,
}

impl EnforceDelegatedPuzzleWrappersSolution {
    /// Creates a new [`EnforceDelegatedPuzzleWrappersSolution`] from the innermost delegated puzzle
    /// hash.
    pub fn new(inner_delegated_puzzle_hash: Bytes32) -> Self {
        Self {
            inner_delegated_puzzle_hash,
        }
    }
}
