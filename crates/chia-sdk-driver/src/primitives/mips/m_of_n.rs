use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{MofNArgs, MofNSolution, NofNArgs, NofNSolution, OneOfNArgs, OneOfNSolution},
    MerkleTree, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{tree_hash_atom, tree_hash_pair, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, Spend, SpendContext};

use super::mips_spend::MipsSpend;

#[derive(Debug, Clone)]
pub struct MofN {
    pub required: usize,
    pub items: Vec<TreeHash>,
}

impl MofN {
    pub fn new(required: usize, items: Vec<TreeHash>) -> Self {
        Self { required, items }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        if self.required == 1 {
            let merkle_tree = self.merkle_tree();
            OneOfNArgs::new(merkle_tree.root()).curry_tree_hash()
        } else if self.required == self.items.len() {
            NofNArgs::new(self.items.clone()).curry_tree_hash()
        } else {
            let merkle_tree = self.merkle_tree();
            MofNArgs::new(self.required, merkle_tree.root()).curry_tree_hash()
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<Spend, DriverError> {
        if self.required == 1 {
            let (member_hash, member_spend) = self
                .items
                .iter()
                .find_map(|item| Some((*item, spend.members.get(item)?)))
                .ok_or(DriverError::MissingSubpathSpend)?;

            let member_spend = member_spend.spend(ctx, spend, delegated_puzzle_wrappers, false)?;

            let merkle_tree = self.merkle_tree();
            let merkle_proof = merkle_tree
                .proof(member_hash.into())
                .ok_or(DriverError::InvalidMerkleProof)?;

            let puzzle = ctx.curry(OneOfNArgs::new(merkle_tree.root()))?;
            let solution = ctx.alloc(&OneOfNSolution::new(
                merkle_proof,
                member_spend.puzzle,
                member_spend.solution,
            ))?;
            Ok(Spend::new(puzzle, solution))
        } else if self.required == self.items.len() {
            let mut puzzles = Vec::with_capacity(self.items.len());
            let mut solutions = Vec::with_capacity(self.items.len());

            for item in &self.items {
                let member = spend
                    .members
                    .get(item)
                    .ok_or(DriverError::MissingSubpathSpend)?;

                let member_spend = member.spend(ctx, spend, delegated_puzzle_wrappers, false)?;

                puzzles.push(member_spend.puzzle);
                solutions.push(member_spend.solution);
            }

            let puzzle = ctx.curry(NofNArgs::new(puzzles))?;
            let solution = ctx.alloc(&NofNSolution::new(solutions))?;
            Ok(Spend::new(puzzle, solution))
        } else {
            let mut puzzle_hashes = Vec::with_capacity(self.required);
            let mut member_spends = HashMap::with_capacity(self.required);

            for &item in &self.items {
                puzzle_hashes.push(item.into());

                let Some(member) = spend.members.get(&item) else {
                    continue;
                };

                member_spends.insert(
                    item,
                    member.spend(ctx, spend, delegated_puzzle_wrappers, false)?,
                );
            }

            if member_spends.len() < self.required {
                return Err(DriverError::InvalidSubpathSpendCount);
            }

            let merkle_tree = self.merkle_tree();
            let proof = m_of_n_proof(ctx, &puzzle_hashes, &member_spends)?;

            let puzzle = ctx.curry(MofNArgs::new(self.required, merkle_tree.root()))?;
            let solution = ctx.alloc(&MofNSolution::new(proof))?;
            Ok(Spend::new(puzzle, solution))
        }
    }

    fn merkle_tree(&self) -> MerkleTree {
        let leaves: Vec<Bytes32> = self.items.iter().map(|&member| member.into()).collect();
        MerkleTree::new(&leaves)
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
