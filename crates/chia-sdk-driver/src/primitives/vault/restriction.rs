use chia_sdk_types::Timelock;
use clvm_utils::TreeHash;

use super::{KnownPuzzles, VaultLayer};

#[derive(Debug, Clone, Copy)]
pub struct Restriction {
    puzzle_hash: TreeHash,
    is_member_condition_validator: bool,
    kind: RestrictionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestrictionKind {
    Timelock(Timelock),
    Unknown,
}

impl Restriction {
    pub fn new(
        puzzle_hash: TreeHash,
        is_member_condition_validator: bool,
        kind: RestrictionKind,
    ) -> Self {
        Self {
            puzzle_hash,
            is_member_condition_validator,
            kind,
        }
    }

    pub fn is_member_condition_validator(&self) -> bool {
        self.is_member_condition_validator
    }

    pub fn kind(&self) -> RestrictionKind {
        self.kind
    }
}

impl VaultLayer for Restriction {
    fn puzzle_hash(&self) -> TreeHash {
        self.puzzle_hash
    }

    fn replace(self, known_puzzles: &KnownPuzzles) -> Self {
        let kind = known_puzzles
            .restrictions
            .get(&self.puzzle_hash)
            .copied()
            .unwrap_or(self.kind);

        Self {
            puzzle_hash: self.puzzle_hash,
            is_member_condition_validator: self.is_member_condition_validator,
            kind,
        }
    }
}
