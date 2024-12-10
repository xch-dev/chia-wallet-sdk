use chia_sdk_types::{DelegatedFeederArgs, IndexWrapperArgs, Mod, RestrictionsArgs};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::{KnownPuzzles, Restriction, VaultLayer};

#[derive(Debug, Clone)]
pub struct PuzzleWithRestrictions<T> {
    nonce: usize,
    restrictions: Vec<Restriction>,
    puzzle: T,
    has_delegated_feeder: bool,
}

impl<T> PuzzleWithRestrictions<T> {
    pub fn top_level(nonce: usize, restrictions: Vec<Restriction>, puzzle: T) -> Self {
        Self {
            nonce,
            restrictions,
            puzzle,
            has_delegated_feeder: true,
        }
    }

    pub fn inner(nonce: usize, restrictions: Vec<Restriction>, puzzle: T) -> Self {
        Self {
            nonce,
            restrictions,
            puzzle,
            has_delegated_feeder: false,
        }
    }
}

impl<T> VaultLayer for PuzzleWithRestrictions<T>
where
    T: VaultLayer,
{
    fn puzzle_hash(&self) -> TreeHash {
        let mut puzzle_hash = self.puzzle.puzzle_hash();

        if !self.restrictions.is_empty() {
            let mut member_validators = Vec::new();
            let mut delegated_puzzle_validators = Vec::new();

            for restriction in &self.restrictions {
                if restriction.is_member_condition_validator() {
                    member_validators.push(restriction.puzzle_hash());
                } else {
                    delegated_puzzle_validators.push(restriction.puzzle_hash());
                }
            }

            puzzle_hash =
                RestrictionsArgs::new(member_validators, delegated_puzzle_validators, puzzle_hash)
                    .curry_tree_hash();
        }

        if self.has_delegated_feeder {
            puzzle_hash = DelegatedFeederArgs::new(puzzle_hash).curry_tree_hash();
        }

        IndexWrapperArgs::new(self.nonce, puzzle_hash).curry_tree_hash()
    }

    fn puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let mut puzzle = self.puzzle.puzzle(ctx)?;

        if !self.restrictions.is_empty() {
            let mut member_validators = Vec::new();
            let mut delegated_puzzle_validators = Vec::new();

            for restriction in &self.restrictions {
                if restriction.is_member_condition_validator() {
                    member_validators.push(restriction.puzzle(ctx)?);
                } else {
                    delegated_puzzle_validators.push(restriction.puzzle(ctx)?);
                }
            }

            puzzle = ctx.curry(RestrictionsArgs::new(
                member_validators,
                delegated_puzzle_validators,
                puzzle,
            ))?;
        }

        if self.has_delegated_feeder {
            puzzle = ctx.curry(DelegatedFeederArgs::new(puzzle))?;
        }

        ctx.curry(IndexWrapperArgs::new(self.nonce, puzzle))
    }

    fn replace(mut self, known_puzzles: &KnownPuzzles) -> Self {
        let mut restrictions = Vec::with_capacity(self.restrictions.len());
        for restriction in self.restrictions {
            restrictions.push(restriction.replace(known_puzzles));
        }
        self.restrictions = restrictions;
        self.puzzle = self.puzzle.replace(known_puzzles);
        self
    }
}
