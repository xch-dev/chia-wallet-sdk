use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct IndexWrapperArgs<I> {
    pub index: usize,
    pub inner_puzzle: I,
}

impl<I> IndexWrapperArgs<I> {
    pub fn new(index: usize, inner_puzzle: I) -> Self {
        Self {
            index,
            inner_puzzle,
        }
    }
}

impl<I> Mod for IndexWrapperArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INDEX_WRAPPER)
    }

    fn mod_hash() -> TreeHash {
        INDEX_WRAPPER_HASH
    }
}

pub const INDEX_WRAPPER: [u8; 7] = hex!("ff02ff05ff0780");

pub const INDEX_WRAPPER_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));
