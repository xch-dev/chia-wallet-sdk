use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{ADD_DPUZ_WRAPPER_HASH, ENFORCE_DPUZ_WRAPPERS, ENFORCE_DPUZ_WRAPPERS_HASH};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::Mod;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EnforceDelegatedPuzzleWrappers {
    pub quoted_add_wrapper_mod_hash: Bytes32,
    pub quoted_wrapper_stack: Vec<Bytes32>,
}

impl EnforceDelegatedPuzzleWrappers {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct EnforceDelegatedPuzzleWrappersSolution {
    pub inner_delegated_puzzle_hash: Bytes32,
}

impl EnforceDelegatedPuzzleWrappersSolution {
    pub fn new(inner_delegated_puzzle_hash: Bytes32) -> Self {
        Self {
            inner_delegated_puzzle_hash,
        }
    }
}
