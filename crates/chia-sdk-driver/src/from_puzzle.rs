use clvmr::{Allocator, NodePtr};

use crate::ParseError;

// given a puzzle, will return info about the coin with that puzzle
pub trait FromPuzzle<A = ()>
where
    Self: Sized,
{
    fn from_puzzle(
        allocator: &mut Allocator,
        puzzle: NodePtr,
        additional_info: A,
    ) -> Result<Self, ParseError>;
}
