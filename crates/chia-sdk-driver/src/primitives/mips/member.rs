use chia_sdk_types::{
    DelegatedFeederArgs, DelegatedFeederSolution, IndexWrapperArgs, Mod, RestrictionsArgs,
    RestrictionsSolution,
};
use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::{
    m_of_n::MofN, member_kind::MemberSpendKind, mips_spend::MipsSpend, restriction::Restriction,
};

#[derive(Debug, Clone)]
pub struct MemberSpend {
    pub nonce: usize,
    pub restrictions: Vec<Restriction>,
    pub kind: MemberSpendKind,
}

impl MemberSpend {
    pub fn new(nonce: usize, restrictions: Vec<Restriction>, spend: Spend) -> Self {
        Self {
            nonce,
            restrictions,
            kind: MemberSpendKind::Leaf(spend),
        }
    }

    pub fn m_of_n(
        nonce: usize,
        restrictions: Vec<Restriction>,
        required: usize,
        items: Vec<TreeHash>,
    ) -> Self {
        Self {
            nonce,
            restrictions,
            kind: MemberSpendKind::MofN(MofN::new(required, items)),
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_spend: bool,
    ) -> Result<Spend, DriverError> {
        let mut result = self.kind.spend(ctx, spend)?;

        if !self.restrictions.is_empty() {
            let mut member_validators = Vec::new();
            let mut delegated_puzzle_validators = Vec::new();

            for restriction in &self.restrictions {
                let restriction_spend = spend
                    .restrictions
                    .get(&restriction.puzzle_hash)
                    .ok_or(DriverError::MissingSubpathSpend)?;

                if restriction.is_member_condition_validator {
                    member_validators.push(restriction_spend.puzzle);
                } else {
                    delegated_puzzle_validators.push(restriction_spend.puzzle);
                }
            }

            let mut member_validator_solutions = Vec::new();
            let mut delegated_puzzle_validator_solutions = Vec::new();

            for restriction in &self.restrictions {
                let restriction_spend = spend
                    .restrictions
                    .get(&restriction.puzzle_hash)
                    .ok_or(DriverError::MissingSubpathSpend)?;

                if restriction.is_member_condition_validator {
                    member_validator_solutions.push(restriction_spend.solution);
                } else {
                    delegated_puzzle_validator_solutions.push(restriction_spend.solution);
                }
            }

            result.puzzle = ctx.curry(RestrictionsArgs::new(
                member_validators,
                delegated_puzzle_validators,
                result.puzzle,
            ))?;

            result.solution = ctx.alloc(&RestrictionsSolution::new(
                member_validator_solutions,
                delegated_puzzle_validator_solutions,
                result.solution,
            ))?;
        }

        if delegated_spend {
            result.puzzle = ctx.curry(DelegatedFeederArgs::new(result.puzzle))?;

            result.solution = ctx.alloc(&DelegatedFeederSolution::new(
                spend.delegated.puzzle,
                spend.delegated.solution,
                result.solution,
            ))?;
        }

        Ok(Spend::new(
            ctx.curry(IndexWrapperArgs::new(self.nonce, result.puzzle))?,
            result.solution,
        ))
    }
}

pub fn member_puzzle_hash(
    nonce: usize,
    restrictions: Vec<Restriction>,
    inner_puzzle_hash: TreeHash,
    top_level: bool,
) -> TreeHash {
    let mut puzzle_hash = inner_puzzle_hash;

    if !restrictions.is_empty() {
        let mut member_validators = Vec::new();
        let mut delegated_puzzle_validators = Vec::new();

        for restriction in restrictions {
            if restriction.is_member_condition_validator {
                member_validators.push(restriction.puzzle_hash);
            } else {
                delegated_puzzle_validators.push(restriction.puzzle_hash);
            }
        }

        puzzle_hash =
            RestrictionsArgs::new(member_validators, delegated_puzzle_validators, puzzle_hash)
                .curry_tree_hash();
    }

    if top_level {
        puzzle_hash = DelegatedFeederArgs::new(puzzle_hash).curry_tree_hash();
    }

    IndexWrapperArgs::new(nonce, puzzle_hash).curry_tree_hash()
}
