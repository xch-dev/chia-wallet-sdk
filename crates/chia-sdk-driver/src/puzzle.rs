use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, SpendContext};

/// A puzzle that has been split into its mod and curried arguments, if it's curried.
///
/// Parsing a puzzle up front computes its hashes once, so each [`Layer::parse_puzzle`] can cheaply
/// check whether the mod hash matches before parsing the curried arguments. Equality compares
/// the full puzzle hash.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{Layer, Puzzle, SpendContext, StandardLayer};
/// use chia_sdk_test::BlsPair;
/// use chia_puzzles::P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH;
///
/// let mut ctx = SpendContext::new();
/// let alice = BlsPair::new(0);
///
/// // The standard puzzle is curried with a public key
/// let p2 = StandardLayer::new(alice.pk);
/// let ptr = p2.construct_puzzle(&mut ctx)?;
/// let puzzle = Puzzle::parse(&ctx, ptr);
///
/// let curried = puzzle.as_curried().expect("the standard puzzle is curried");
/// assert_eq!(curried.mod_hash, P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE_HASH.into());
/// assert_eq!(puzzle.curried_puzzle_hash(), alice.puzzle_hash.into());
///
/// // A puzzle that isn't curried is raw, and its mod hash is its puzzle hash
/// let ptr = ctx.alloc(&1)?;
/// let puzzle = Puzzle::parse(&ctx, ptr);
/// assert!(puzzle.is_raw());
/// assert_eq!(puzzle.mod_hash(), puzzle.curried_puzzle_hash());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy)]
pub enum Puzzle {
    /// A puzzle of the form `(a (q . MOD) ARGS)`, where `ARGS` are the curried arguments.
    Curried(CurriedPuzzle),
    /// A puzzle that isn't curried.
    Raw(RawPuzzle),
}

impl Puzzle {
    /// Parses a puzzle, as [`Puzzle::Curried`] if it's curried and [`Puzzle::Raw`] otherwise.
    ///
    /// This never fails, and computes the tree hashes of the puzzle and its mod.
    pub fn parse(allocator: &Allocator, puzzle: NodePtr) -> Self {
        CurriedPuzzle::parse(allocator, puzzle).map_or_else(
            || {
                Self::Raw(RawPuzzle {
                    puzzle_hash: tree_hash(allocator, puzzle),
                    ptr: puzzle,
                })
            },
            Self::Curried,
        )
    }

    /// The tree hash of the full puzzle, including any curried arguments. This is the puzzle hash
    /// of a coin with this puzzle.
    pub fn curried_puzzle_hash(&self) -> TreeHash {
        match self {
            Self::Curried(curried) => curried.curried_puzzle_hash,
            Self::Raw(raw) => raw.puzzle_hash,
        }
    }

    /// The tree hash of the uncurried mod. For a raw puzzle, this is the puzzle hash.
    pub fn mod_hash(&self) -> TreeHash {
        match self {
            Self::Curried(curried) => curried.mod_hash,
            Self::Raw(raw) => raw.puzzle_hash,
        }
    }

    /// The pointer to the full puzzle, including any curried arguments.
    pub fn ptr(&self) -> NodePtr {
        match self {
            Self::Curried(curried) => curried.curried_ptr,
            Self::Raw(raw) => raw.ptr,
        }
    }

    /// Whether this is a [`Puzzle::Curried`].
    pub fn is_curried(&self) -> bool {
        matches!(self, Self::Curried(_))
    }

    /// Whether this is a [`Puzzle::Raw`].
    pub fn is_raw(&self) -> bool {
        matches!(self, Self::Raw(_))
    }

    /// Returns the [`CurriedPuzzle`], or [`None`] if the puzzle isn't curried.
    pub fn as_curried(&self) -> Option<CurriedPuzzle> {
        match self {
            Self::Curried(curried) => Some(*curried),
            Self::Raw(_raw) => None,
        }
    }

    /// Returns the [`RawPuzzle`], or [`None`] if the puzzle is curried.
    pub fn as_raw(&self) -> Option<RawPuzzle> {
        match self {
            Self::Curried(_curried) => None,
            Self::Raw(raw) => Some(*raw),
        }
    }
}

impl PartialEq for Puzzle {
    fn eq(&self, other: &Self) -> bool {
        self.curried_puzzle_hash() == other.curried_puzzle_hash()
    }
}

impl Eq for Puzzle {}

impl FromClvm<Allocator> for Puzzle {
    fn from_clvm(allocator: &Allocator, puzzle: NodePtr) -> Result<Self, FromClvmError> {
        Ok(Self::parse(allocator, puzzle))
    }
}

/// A curried puzzle, split into its mod and curried arguments.
#[derive(Debug, Clone, Copy)]
pub struct CurriedPuzzle {
    /// The tree hash of the full puzzle, including the curried arguments.
    pub curried_puzzle_hash: TreeHash,
    /// The pointer to the full puzzle, including the curried arguments.
    pub curried_ptr: NodePtr,
    /// The tree hash of the mod that the arguments are curried into.
    pub mod_hash: TreeHash,
    /// The pointer to the curried arguments, which is a CLVM list in the curried form.
    /// These can be parsed with a type that derives `FromClvm` with `#[clvm(curry)]`.
    pub args: NodePtr,
}

impl CurriedPuzzle {
    /// Parses a curried puzzle, returning [`None`] if the puzzle isn't curried.
    pub fn parse(allocator: &Allocator, puzzle: NodePtr) -> Option<Self> {
        let curried = CurriedProgram::from_clvm(allocator, puzzle).ok()?;
        let mod_hash = tree_hash(allocator, curried.program);
        let curried_puzzle_hash = CurriedProgram {
            program: mod_hash,
            args: tree_hash(allocator, curried.args),
        }
        .tree_hash();

        Some(Self {
            curried_puzzle_hash,
            curried_ptr: puzzle,
            mod_hash,
            args: curried.args,
        })
    }
}

/// A puzzle that isn't curried.
#[derive(Debug, Clone, Copy)]
pub struct RawPuzzle {
    /// The tree hash of the puzzle.
    pub puzzle_hash: TreeHash,
    /// The pointer to the puzzle.
    pub ptr: NodePtr,
}

impl ToTreeHash for Puzzle {
    fn tree_hash(&self) -> TreeHash {
        self.curried_puzzle_hash()
    }
}

impl Layer for Puzzle {
    type Solution = NodePtr;

    fn parse_puzzle(_allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        Ok(Some(puzzle))
    }

    fn parse_solution(
        _allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(solution)
    }

    fn construct_puzzle(&self, _ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(self.ptr())
    }

    fn construct_solution(
        &self,
        _ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        Ok(solution)
    }
}
