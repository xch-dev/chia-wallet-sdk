use chia_protocol::Bytes32;
use chia_sdk_types::{MerkleTree, Mod, Vault1ofNArgs, VaultMofNArgs, VaultNofNArgs};
use clvm_utils::TreeHash;

use super::{KnownPuzzles, Member, PuzzleWithRestrictions, VaultLayer};

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
        if self.required == 1 {
            let merkle_tree = self.merkle_tree();
            Vault1ofNArgs::new(merkle_tree.root()).curry_tree_hash()
        } else if self.required == self.members.len() {
            let members = self.members.iter().map(VaultLayer::puzzle_hash).collect();
            VaultNofNArgs::new(members).curry_tree_hash()
        } else {
            let merkle_tree = self.merkle_tree();
            VaultMofNArgs::new(self.required, merkle_tree.root()).curry_tree_hash()
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
