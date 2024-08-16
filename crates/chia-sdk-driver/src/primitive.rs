use chia_protocol::Coin;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle};

/// A trait for defining a full primitive type that can be spent.
/// This is made up of various puzzle layers.
pub trait Primitive {
    /// Parses the full information required to spend a coin of this primitive type.
    /// If it's not a match, it will return [`None`].
    /// If it should be a match but an error occurs, it will return that error.
    ///
    /// This can involve parsing the parent puzzle and solution, and possibly running it
    /// in order to get the output conditions, which can be used to derive further information.
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized;
}
