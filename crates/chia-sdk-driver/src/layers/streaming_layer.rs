use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_sdk_types::Mod;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{CurriedPuzzle, DriverError, Layer, Puzzle, SpendContext};

/// The streaming [`Layer`] pays out the value of a coin to a recipient linearly over time.
///
/// Each spend pays the recipient the share of the remaining amount that has vested since the last
/// payment time, and recreates the coin with the rest and the new last payment time. A spend must
/// be authorized by a message from the recipient, or from the clawback puzzle hash if there is one.
/// When clawed back, the amount that hasn't vested is sent to the clawback puzzle hash instead of
/// recreating the stream.
///
/// The puzzle is curried twice, so that it can recreate itself with a new last payment time. The
/// first curry contains the values that never change, and the second contains the hash of the
/// first curry and the last payment time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamLayer {
    /// The puzzle hash that receives the payments.
    pub recipient: Bytes32,
    /// The puzzle hash that can claw back the amount that hasn't vested yet, if any.
    pub clawback_ph: Option<Bytes32>,
    /// The timestamp at which the full amount has vested.
    pub end_time: u64,
    /// The timestamp up to which the recipient has been paid. Initially, this is the start time.
    pub last_payment_time: u64,
}

impl StreamLayer {
    /// Creates a [`StreamLayer`] from its curried values.
    pub fn new(
        recipient: Bytes32,
        clawback_ph: Option<Bytes32>,
        end_time: u64,
        last_payment_time: u64,
    ) -> Self {
        Self {
            recipient,
            clawback_ph,
            end_time,
            last_payment_time,
        }
    }

    /// Calculates the puzzle hash of the stream without allocating it.
    pub fn puzzle_hash(&self) -> TreeHash {
        StreamPuzzle2ndCurryArgs::curry_tree_hash(
            self.recipient,
            self.clawback_ph,
            self.end_time,
            self.last_payment_time,
        )
    }
}

impl Layer for StreamLayer {
    type Solution = StreamPuzzleSolution;

    fn parse_puzzle(
        allocator: &Allocator,
        puzzle_2nd_curry: Puzzle,
    ) -> Result<Option<Self>, DriverError> {
        let Some(puzzle_2nd_curry) = puzzle_2nd_curry.as_curried() else {
            return Ok(None);
        };

        let Ok(program_2nd_curry) =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, puzzle_2nd_curry.curried_ptr)
        else {
            return Ok(None);
        };
        let Some(puzzle_1st_curry) = CurriedPuzzle::parse(allocator, program_2nd_curry.program)
        else {
            return Ok(None);
        };

        if puzzle_1st_curry.mod_hash != STREAM_PUZZLE_HASH {
            return Ok(None);
        }

        let Ok(args1) = StreamPuzzle1stCurryArgs::from_clvm(allocator, puzzle_1st_curry.args)
        else {
            return Ok(None);
        };
        let Ok(args2) = StreamPuzzle2ndCurryArgs::from_clvm(allocator, puzzle_2nd_curry.args)
        else {
            return Ok(None);
        };

        // The puzzle uses SELF_HASH to recreate itself with the remaining amount after a payment.
        // If it doesn't match the first curry, the remainder would be sent to a different puzzle.
        if args2.self_hash != puzzle_2nd_curry.mod_hash.into() {
            return Ok(None);
        }

        Ok(Some(Self {
            recipient: args1.recipient,
            clawback_ph: args1.clawback_ph,
            end_time: args1.end_time,
            last_payment_time: args2.last_payment_time,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        StreamPuzzleSolution::from_clvm(allocator, solution).map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let puzzle_1st_curry = ctx.curry(StreamPuzzle1stCurryArgs::new(
            self.recipient,
            self.clawback_ph,
            self.end_time,
        ))?;
        let self_hash = StreamPuzzle1stCurryArgs::curry_tree_hash(
            self.recipient,
            self.clawback_ph,
            self.end_time,
        );

        ctx.alloc(&CurriedProgram {
            program: puzzle_1st_curry,
            args: StreamPuzzle2ndCurryArgs::new(self_hash.into(), self.last_payment_time),
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

/// The serialized streaming puzzle, before either curry.
pub const STREAM_PUZZLE: [u8; 587] = hex!(
    "ff02ffff01ff02ffff03ffff09ff8202ffffff05ffff14ffff12ff81bfffff11ff82017fff5f8080ffff11ff17ff5f80808080ffff01ff04ffff04ff18ffff04ff81bfff808080ffff04ffff03ff8203ffffff04ff10ffff04ff82017fff808080ffff04ff14ffff04ff82017fff80808080ffff04ffff03ffff09ff8202ffff8080ffff04ff1aff8080ffff04ff1cffff04ff05ffff04ff8202ffffff04ffff04ff05ff8080ff808080808080ffff04ffff03ffff09ff81bfff8202ff80ffff04ff1aff8080ffff04ff1cffff04ffff03ff8203ffff0bffff0bff5effff0bff16ffff0bff16ff6eff2f80ffff0bff16ffff0bff7effff0bff16ffff0bff16ff6effff0bffff0101ff2f8080ffff0bff16ffff0bff7effff0bff16ffff0bff16ff6effff0bffff0101ff82017f8080ffff0bff16ff6eff4e808080ff4e808080ff4e80808080ffff04ffff11ff81bfff8202ff80ffff04ffff04ffff0bffff0173ff0580ff8080ff808080808080ffff04ffff04ff12ffff04ffff0117ffff04ff82017fffff04ffff03ff8203ffff0bff0580ff8080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffffff5549ff5133ffff4301ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff018080"
);
/// The tree hash of [`STREAM_PUZZLE`].
pub const STREAM_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "e0e312a612aa14357e225c0dc21d351610c2377efab14406da6c7424d48feff8"
));

/// The first curried arguments of the streaming puzzle, which never change.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct StreamPuzzle1stCurryArgs {
    /// The puzzle hash that receives the payments.
    pub recipient: Bytes32,
    /// The puzzle hash that can claw back the amount that hasn't vested yet, if any.
    pub clawback_ph: Option<Bytes32>,
    /// The timestamp at which the full amount has vested.
    pub end_time: u64,
}

impl StreamPuzzle1stCurryArgs {
    /// Creates the first curried arguments.
    pub fn new(recipient: Bytes32, clawback_ph: Option<Bytes32>, end_time: u64) -> Self {
        Self {
            recipient,
            clawback_ph,
            end_time,
        }
    }

    /// Calculates the tree hash of [`STREAM_PUZZLE`] with the first arguments curried in. This is
    /// the `self_hash` of the second curry.
    pub fn curry_tree_hash(
        recipient: Bytes32,
        clawback_ph: Option<Bytes32>,
        end_time: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: STREAM_PUZZLE_HASH,
            args: StreamPuzzle1stCurryArgs::new(recipient, clawback_ph, end_time),
        }
        .tree_hash()
    }
}

/// The second curried arguments of the streaming puzzle, which are curried into the puzzle with
/// the first arguments already curried in.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct StreamPuzzle2ndCurryArgs {
    /// The tree hash of the puzzle with the first arguments curried in, which the puzzle uses to
    /// recreate itself.
    pub self_hash: Bytes32,
    /// The timestamp up to which the recipient has been paid.
    pub last_payment_time: u64,
}

impl StreamPuzzle2ndCurryArgs {
    /// Creates the second curried arguments.
    pub fn new(self_hash: Bytes32, last_payment_time: u64) -> Self {
        Self {
            self_hash,
            last_payment_time,
        }
    }

    /// Calculates the full puzzle hash of a stream, with both curries.
    pub fn curry_tree_hash(
        recipient: Bytes32,
        clawback_ph: Option<Bytes32>,
        end_time: u64,
        last_payment_time: u64,
    ) -> TreeHash {
        let self_hash = StreamPuzzle1stCurryArgs::curry_tree_hash(recipient, clawback_ph, end_time);
        CurriedProgram {
            program: self_hash,
            args: StreamPuzzle2ndCurryArgs::new(self_hash.into(), last_payment_time),
        }
        .tree_hash()
    }
}

/// The solution of the streaming puzzle.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Copy, Eq)]
#[clvm(list)]
pub struct StreamPuzzleSolution {
    /// The amount of the coin being spent, which is asserted by the puzzle.
    pub my_amount: u64,
    /// The time to pay the recipient up to, which is asserted by the puzzle.
    pub payment_time: u64,
    /// The amount paid to the recipient, which must be the share of `my_amount` that vested
    /// between the last payment time and the payment time.
    pub to_pay: u64,
    /// Whether the spend is a clawback, which is authorized by the clawback puzzle hash rather
    /// than the recipient.
    #[clvm(rest)]
    pub clawback: bool,
}

impl Mod for StreamPuzzle1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&STREAM_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        STREAM_PUZZLE_HASH
    }
}
