// use clvm_traits::{FromClvm, ToClvm};
// use clvm_utils::TreeHash;

// use crate::Mod;

// use super::Force1of2RestrictedVariable;

// #[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
// #[clvm(transparent)]
// pub enum Restriction {
//     Force1of2RestrictedVariable(Force1of2RestrictedVariable),
// }

// impl Restriction {
//     pub fn curry_tree_hash(&self) -> TreeHash {
//         match self {}
//     }
// }
