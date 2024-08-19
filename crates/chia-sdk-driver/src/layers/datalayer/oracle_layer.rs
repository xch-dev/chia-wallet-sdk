use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Condition, CreateCoin, CreatePuzzleAnnouncement};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr, SExp};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, SpendContext};

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
    pub fn new(oracle_puzzle_hash: Bytes32, oracle_fee: u64) -> Self {
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

        let quote_pair = allocator.sexp(puzzle.ptr);
        let SExp::Pair(one_ptr, condition_list_ptr) = quote_pair else {
            return Ok(None);
        };
        if !allocator
            .small_number(one_ptr)
            .map(|one| one == 1)
            .unwrap_or(false)
        {
            return Ok(None);
        }

        let conditions = Vec::<Condition<NodePtr>>::from_clvm(allocator, condition_list_ptr)?;
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

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
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
            .to_clvm(ctx.allocator_mut())
            .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        _: &mut SpendContext,
        _: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(NodePtr::NIL)
    }
}

pub const WRITER_FILTER_PUZZLE: [u8; 110] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ffff02ff05ff0b80ff80808080ffff04ffff01ff02ffff
    03ff05ffff01ff02ffff03ffff09ff11ffff0181f380ffff01ff0880ffff01ff04ff09ffff02ff02
    ffff04ff02ffff04ff0dff808080808080ff0180ff8080ff0180ff018080
    "
);

pub const WRITER_FILTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    407f70ea751c25052708219ae148b45db2f61af2287da53d600b2486f12b3ca6
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct WriterLayerArgs<I> {
    pub inner_puzzle: I,
}

impl<I> WriterLayerArgs<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl WriterLayerArgs<TreeHash> {
    pub fn curry_tree_hash(inner_puzzle: TreeHash) -> TreeHash {
        CurriedProgram {
            program: WRITER_FILTER_PUZZLE_HASH,
            args: WriterLayerArgs { inner_puzzle },
        }
        .tree_hash()
    }
}
