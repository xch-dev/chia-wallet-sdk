use chia_puzzles::{AUGMENTED_CONDITION, AUGMENTED_CONDITION_HASH};

use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{Condition, Mod};

/// Curried arguments for the augmented condition puzzle, which adds a fixed condition to the
/// output of an inner puzzle.
///
/// When spent, it runs the inner puzzle with the inner solution and prepends `condition` to the
/// result. This is useful for adding a restriction such as a timelock to a puzzle without
/// changing the puzzle itself.
///
/// ## Example
///
/// ```rust
/// # use chia_sdk_types::{Mod, conditions::AssertSecondsAbsolute, puzzles::AugmentedConditionArgs};
/// # use clvm_utils::TreeHash;
/// // An inner puzzle that can only be spent after a certain time.
/// let args = AugmentedConditionArgs::<TreeHash, TreeHash>::new(
///     AssertSecondsAbsolute::new(1_700_000_000).into(),
///     TreeHash::new([1; 32]),
/// );
/// let puzzle_hash = args.curry_tree_hash();
/// # let _ = puzzle_hash;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AugmentedConditionArgs<T, I> {
    /// The condition to prepend to the inner puzzle's output.
    pub condition: Condition<T>,
    /// The inner puzzle, which is run with the inner solution.
    pub inner_puzzle: I,
}

impl<T, I> AugmentedConditionArgs<T, I> {
    /// Creates new [`AugmentedConditionArgs`] from the condition and inner puzzle.
    pub fn new(condition: Condition<T>, inner_puzzle: I) -> Self {
        Self {
            condition,
            inner_puzzle,
        }
    }
}

impl<T, I> Mod for AugmentedConditionArgs<T, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUGMENTED_CONDITION)
    }

    fn mod_hash() -> TreeHash {
        AUGMENTED_CONDITION_HASH.into()
    }
}

/// The solution for the augmented condition puzzle. See [`AugmentedConditionArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AugmentedConditionSolution<I> {
    /// The solution for the inner puzzle.
    pub inner_solution: I,
}

impl<I> AugmentedConditionSolution<I> {
    /// Creates a new [`AugmentedConditionSolution`] from the inner solution.
    pub fn new(inner_solution: I) -> Self {
        Self { inner_solution }
    }
}
