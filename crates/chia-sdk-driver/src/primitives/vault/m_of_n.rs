use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    MerkleTree, Mod, Vault1ofNArgs, Vault1ofNSolution, VaultMofNArgs, VaultMofNSolution,
    VaultNofNArgs, VaultNofNSolution,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{tree_hash_atom, tree_hash_pair, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, Spend, SpendContext};

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

    pub fn solve(
        &self,
        ctx: &mut SpendContext,
        member_spends: HashMap<TreeHash, Spend>,
    ) -> Result<NodePtr, DriverError> {
        if member_spends.len() != self.required {
            return Err(DriverError::WrongSpendCount);
        }

        match self.optimization() {
            Some(MofNOptimization::One) => {
                let (member_puzzle_hash, member_spend) = member_spends
                    .into_iter()
                    .next()
                    .expect("missing single spend");

                let merkle_tree = self.merkle_tree();
                let merkle_proof = merkle_tree
                    .proof(member_puzzle_hash.into())
                    .ok_or(DriverError::InvalidMerkleProof)?;

                ctx.alloc(&Vault1ofNSolution::new(
                    merkle_proof,
                    member_spend.puzzle,
                    member_spend.solution,
                ))
            }
            Some(MofNOptimization::All) => {
                let mut member_solutions = Vec::with_capacity(self.required);

                for member in &self.members {
                    let spend = member_spends
                        .get(&member.puzzle_hash())
                        .ok_or(DriverError::MissingMemberSpend)?;

                    member_solutions.push(spend.solution);
                }

                ctx.alloc(&VaultNofNSolution::new(member_solutions))
            }
            None => {
                let puzzle_hashes: Vec<Bytes32> = self
                    .members
                    .iter()
                    .map(|member| member.puzzle_hash().into())
                    .collect();

                let proof = m_of_n_proof(ctx, &puzzle_hashes, &member_spends)?;

                ctx.alloc(&VaultMofNSolution::new(proof))
            }
        }
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

fn m_of_n_proof(
    ctx: &mut SpendContext,
    puzzle_hashes: &[Bytes32],
    member_spends: &HashMap<TreeHash, Spend>,
) -> Result<NodePtr, DriverError> {
    if puzzle_hashes.len() == 1 {
        let puzzle_hash = puzzle_hashes[0];

        return if let Some(spend) = member_spends.get(&puzzle_hash.into()) {
            ctx.alloc(&clvm_tuple!((), spend.puzzle, spend.solution))
        } else {
            ctx.alloc(&Bytes32::from(tree_hash_atom(&puzzle_hash)))
        };
    }

    let mid_index = puzzle_hashes.len().div_ceil(2);
    let first = &puzzle_hashes[..mid_index];
    let rest = &puzzle_hashes[mid_index..];

    let first_proof = m_of_n_proof(ctx, first, member_spends)?;
    let rest_proof = m_of_n_proof(ctx, rest, member_spends)?;

    if first_proof.is_pair() || rest_proof.is_pair() {
        ctx.alloc(&(first_proof, rest_proof))
    } else {
        let first_hash = ctx.extract::<Bytes32>(first_proof)?;
        let rest_hash = ctx.extract::<Bytes32>(rest_proof)?;
        let pair_hash = Bytes32::from(tree_hash_pair(first_hash.into(), rest_hash.into()));
        ctx.alloc(&pair_hash)
    }
}
