use chia_protocol::Bytes32;
use chia_sdk_types::{MerkleTree, Mod, Vault1ofNArgs, VaultMofNArgs, VaultNofNArgs};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::{KnownPuzzles, Member, PuzzleWithRestrictions, VaultLayer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MofNOptimization {
    One,
    All,
}

#[derive(Debug, Clone)]
pub struct MofN {
    required: usize,
    members: Vec<PuzzleWithRestrictions<Member>>,
}

impl MofN {
    pub fn new(required: usize, members: Vec<PuzzleWithRestrictions<Member>>) -> Option<Self> {
        if members.len() < required {
            return None;
        }
        Some(Self { required, members })
    }

    pub fn required(&self) -> usize {
        self.required
    }

    pub fn members(&self) -> &[PuzzleWithRestrictions<Member>] {
        &self.members
    }

    fn optimization(&self) -> Option<MofNOptimization> {
        if self.required == 1 {
            Some(MofNOptimization::One)
        } else if self.required == self.members.len() {
            Some(MofNOptimization::All)
        } else {
            None
        }
    }

    fn merkle_tree(&self) -> MerkleTree {
        let leaves: Vec<Bytes32> = self
            .members
            .iter()
            .map(|member| member.puzzle_hash().into())
            .collect();
        MerkleTree::new(&leaves)
    }
}

impl VaultLayer for MofN {
    fn puzzle_hash(&self) -> TreeHash {
        match self.optimization() {
            Some(MofNOptimization::One) => {
                let merkle_tree = self.merkle_tree();
                Vault1ofNArgs::new(merkle_tree.root()).curry_tree_hash()
            }
            Some(MofNOptimization::All) => {
                let members = self.members.iter().map(VaultLayer::puzzle_hash).collect();
                VaultNofNArgs::new(members).curry_tree_hash()
            }
            None => {
                let merkle_tree = self.merkle_tree();
                VaultMofNArgs::new(self.required, merkle_tree.root()).curry_tree_hash()
            }
        }
    }

    fn puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        match self.optimization() {
            Some(MofNOptimization::One) => {
                let merkle_tree = self.merkle_tree();
                ctx.curry(Vault1ofNArgs::new(merkle_tree.root()))
            }
            Some(MofNOptimization::All) => {
                let members = self
                    .members
                    .iter()
                    .map(|member| member.puzzle(ctx))
                    .collect::<Result<_, _>>()?;
                ctx.curry(VaultNofNArgs::new(members))
            }
            None => {
                let merkle_tree = self.merkle_tree();
                ctx.curry(VaultMofNArgs::new(self.required, merkle_tree.root()))
            }
        }
    }

    fn replace(self, known_puzzles: &KnownPuzzles) -> Self {
        let required = self.required;
        let members = self
            .members
            .into_iter()
            .map(|member| member.replace(known_puzzles))
            .collect();
        Self { required, members }
    }
}
