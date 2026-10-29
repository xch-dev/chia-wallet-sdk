use clvmr::NodePtr;

use crate::{Cat, Puzzle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedCat {
    pub cat: Cat,
    pub p2_puzzle: Puzzle,
    pub p2_solution: NodePtr,
    pub revoked: bool,
}
