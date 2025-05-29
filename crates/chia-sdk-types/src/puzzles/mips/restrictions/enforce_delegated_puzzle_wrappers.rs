use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::ADD_DELEGATED_PUZZLE_WRAPPER_PUZZLE_HASH, Mod};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EnforceDelegatedPuzzleWrappers {
    pub quoted_add_wrapper_mod_hash: Bytes32,
    pub quoted_wrapper_stack: Vec<Bytes32>,
}

impl EnforceDelegatedPuzzleWrappers {
    pub fn new(wrapper_stack: &[TreeHash]) -> Self {
        Self {
            quoted_add_wrapper_mod_hash: clvm_quote!(ADD_DELEGATED_PUZZLE_WRAPPER_PUZZLE_HASH)
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
        Cow::Borrowed(&ENFORCE_DELEGATED_PUZZLE_WRAPPERS_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ENFORCE_DELEGATED_PUZZLE_WRAPPERS_PUZZLE_HASH
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
pub const ENFORCE_DELEGATED_PUZZLE_WRAPPERS_PUZZLE: [u8; 363] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff17ffff02ff1effff04ff02ffff04ff05ffff
    04ff0bffff04ff2fff80808080808080ff80ffff01ff088080ff0180ffff04ff
    ff01ffffa0ba4484b961b7a2369d948d06c55b64bdbfaffb326bc13b490ab121
    5dd33d8d46ffa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d
    78f596718ba7b2a0a12871fee210fb8619291eaea194581cbd2531e4b23759d2
    25f6806923f63222ffffa0a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e72
    98a91ce119a63400ade7c5a04bf5122f344554c53bde2ebb8cd2b7e3d1600ad6
    31c385a5d7cce23c7785459aff02ff02ffff03ff0bffff01ff0bff16ff1cffff
    0bff16ff05ffff0bff16ffff0bff16ff12ffff0bff16ff13ffff0bff16ffff0b
    ff16ff12ffff0bff16ffff0bff16ff14ffff02ff1effff04ff02ffff04ff05ff
    ff04ff1bffff04ff17ff80808080808080ff088080ff1a808080ff1a808080ff
    ff011780ff0180ff018080
    "
);

pub const ENFORCE_DELEGATED_PUZZLE_WRAPPERS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "1f94aa2381c1c02fec90687c0b045ef3cad4b458f8eac5bd90695b4d89624f09"
));
