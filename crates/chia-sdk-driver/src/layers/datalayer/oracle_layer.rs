use core::panic;

use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Condition, CreateCoin, CreatePuzzleAnnouncement};
use clvm_traits::{clvm_quote, match_quote, FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The Oracle [`Layer`] enables anyone to spend a coin provided they pay an XCH fee to an address.
/// It's typically used with [`DelegationLayer`](crate::DelegationLayer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleLayer {
    /// The puzzle hash corresponding to the address the fee should be paid to.
    pub oracle_puzzle_hash: Bytes32,
    /// The amount of XCH that should be paid to the oracle.
    pub oracle_fee: u64,
}

impl OracleLayer {
    #[allow(clippy::missing_panics_doc)]
    pub fn new(oracle_puzzle_hash: Bytes32, oracle_fee: u64) -> Self {
        assert!(oracle_fee % 2 == 0, "oracle fee must be even");

        Self {
            oracle_puzzle_hash,
            oracle_fee,
        }
    }
}

impl Layer for OracleLayer {
    type Solution = ();

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_raw() else {
            return Ok(None);
        };

        let (_q, conditions) =
            <match_quote!(Vec<Condition<NodePtr>>)>::from_clvm(allocator, puzzle.ptr)?;
        if conditions.len() != 2 {
            return Ok(None);
        }

        if let Some(Condition::CreateCoin(create_coin)) = conditions.first() {
            Ok(Some(Self::new(create_coin.puzzle_hash, create_coin.amount)))
        } else {
            Ok(None)
        }
    }

    fn parse_solution(_: &Allocator, _: NodePtr) -> Result<Self::Solution, DriverError> {
        Ok(())
    }

    #[allow(clippy::missing_panics_doc)]
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        assert!(self.oracle_fee % 2 == 0, "oracle fee must be even");

        // first condition: (list CREATE_COIN oracle_puzzle_hash oracle_fee)
        // second condition: (list CREATE_PUZZLE_ANNOUNCEMENT '$')

        let conditions: Vec<Condition<NodePtr>> = vec![
            Condition::CreateCoin(CreateCoin {
                puzzle_hash: self.oracle_puzzle_hash,
                amount: self.oracle_fee,
                memos: vec![],
            }),
            Condition::CreatePuzzleAnnouncement(CreatePuzzleAnnouncement {
                message: Bytes::new("$".into()),
            }),
        ];

        clvm_quote!(conditions)
            .to_clvm(&mut ctx.allocator)
            .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        _: &mut SpendContext,
        (): Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(NodePtr::NIL)
    }
}

impl OracleLayer {
    pub fn spend(self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, ())?;

        Ok(Spend { puzzle, solution })
    }
}
