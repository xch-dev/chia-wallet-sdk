use chia_protocol::Bytes32;
use chia_sdk_types::{
    MerkleTree, Mod,
    puzzles::{MofNArgs, MofNSolution, NofNArgs, NofNSolution, OneOfNArgs, OneOfNSolution},
};
use clvm_traits::clvm_tuple;
use clvm_utils::{TreeHash, tree_hash_atom, tree_hash_pair};
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

    /// Checks that `required` is between 1 and the number of items. Otherwise, the puzzle either
    /// can be spent by anyone or can't be spent at all.
    pub fn validate(&self) -> Result<(), DriverError> {
        if self.required == 0 || self.required > self.items.len() {
            return Err(DriverError::InvalidMofNRequired {
                required: self.required,
                items: self.items.len(),
            });
        }

        Ok(())
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<Spend, DriverError> {
        let selected = self.selected_items(spend)?;

        if self.required == 1 {
            let member_hash = self.items[selected[0]];

            let member_spend =
                spend.members[&member_hash].spend(ctx, spend, delegated_puzzle_wrappers, false)?;

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
                let member_spend =
                    spend.members[item].spend(ctx, spend, delegated_puzzle_wrappers, false)?;

                puzzles.push(member_spend.puzzle);
                solutions.push(member_spend.solution);
            }

            let puzzle = ctx.curry(NofNArgs::new(puzzles))?;
            let solution = ctx.alloc(&NofNSolution::new(solutions))?;
            Ok(Spend::new(puzzle, solution))
        } else {
            let mut leaves: Vec<(Bytes32, Option<Spend>)> =
                self.items.iter().map(|&item| (item.into(), None)).collect();

            for index in selected {
                let member_spend = spend.members[&self.items[index]].spend(
                    ctx,
                    spend,
                    delegated_puzzle_wrappers,
                    false,
                )?;

                leaves[index].1 = Some(member_spend);
            }

            let merkle_tree = self.merkle_tree();
            let proof = m_of_n_proof(ctx, &leaves)?;

            let puzzle = ctx.curry(MofNArgs::new(self.required, merkle_tree.root()))?;
            let solution = ctx.alloc(&MofNSolution::new(proof))?;
            Ok(Spend::new(puzzle, solution))
        }
    }

    /// Merges the delegated puzzle wrappers of the members that [`MofN::spend`] would spend into
    /// the stack.
    pub fn collect_delegated_puzzle_wrappers(
        &self,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<(), DriverError> {
        for index in self.selected_items(spend)? {
            spend.members[&self.items[index]]
                .collect_delegated_puzzle_wrappers(spend, delegated_puzzle_wrappers)?;
        }

        Ok(())
    }

    /// Returns the indices of the items that will be spent, which are the first `required` items
    /// that have a member spend. The M of N puzzle requires exactly `required` members to be spent.
    fn selected_items(&self, spend: &MipsSpend) -> Result<Vec<usize>, DriverError> {
        self.validate()?;

        let selected: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| spend.members.contains_key(item))
            .map(|(index, _)| index)
            .take(self.required)
            .collect();

        if selected.len() < self.required {
            return Err(if self.required == 1 || self.required == self.items.len() {
                DriverError::MissingSubpathSpend
            } else {
                DriverError::InvalidSubpathSpendCount
            });
        }

        Ok(selected)
    }

    fn merkle_tree(&self) -> MerkleTree {
        let leaves: Vec<Bytes32> = self.items.iter().map(|&member| member.into()).collect();
        MerkleTree::new(&leaves)
    }
}

fn m_of_n_proof(
    ctx: &mut SpendContext,
    leaves: &[(Bytes32, Option<Spend>)],
) -> Result<NodePtr, DriverError> {
    if let [(puzzle_hash, spend)] = leaves {
        return if let Some(spend) = spend {
            ctx.alloc(&clvm_tuple!((), spend.puzzle, spend.solution))
        } else {
            ctx.alloc(&Bytes32::from(tree_hash_atom(puzzle_hash)))
        };
    }

    let mid_index = leaves.len().div_ceil(2);
    let first = &leaves[..mid_index];
    let rest = &leaves[mid_index..];

    let first_proof = m_of_n_proof(ctx, first)?;
    let rest_proof = m_of_n_proof(ctx, rest)?;

    if first_proof.is_pair() || rest_proof.is_pair() {
        ctx.alloc(&(first_proof, rest_proof))
    } else {
        let first_hash = ctx.extract::<Bytes32>(first_proof)?;
        let rest_hash = ctx.extract::<Bytes32>(rest_proof)?;
        let pair_hash = Bytes32::from(tree_hash_pair(first_hash.into(), rest_hash.into()));
        ctx.alloc(&pair_hash)
    }
}
