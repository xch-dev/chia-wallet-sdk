use chia_sdk_types::{
    puzzles::{
        AddDelegatedPuzzleWrapper, AddDelegatedPuzzleWrapperSolution, DelegatedFeederArgs,
        DelegatedFeederSolution, EnforceDelegatedPuzzleWrappers,
        EnforceDelegatedPuzzleWrappersSolution, IndexWrapperArgs, RestrictionsArgs,
        RestrictionsSolution,
    },
    Mod,
};
use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::{
    m_of_n::MofN, mips_spend::MipsSpend, restriction::Restriction, MipsSpendKind, RestrictionKind,
};

#[derive(Debug, Clone)]
pub struct InnerPuzzleSpend {
    pub nonce: usize,
    pub restrictions: Vec<Restriction>,
    pub kind: MipsSpendKind,
}

impl InnerPuzzleSpend {
    pub fn new(nonce: usize, restrictions: Vec<Restriction>, spend: Spend) -> Self {
        Self {
            nonce,
            restrictions,
            kind: MipsSpendKind::Member(spend),
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
            kind: MipsSpendKind::MofN(MofN::new(required, items)),
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
        delegated_spend: bool,
    ) -> Result<Spend, DriverError> {
        let mut result = self.kind.spend(ctx, spend, delegated_puzzle_wrappers)?;

        if !self.restrictions.is_empty() {
            let mut member_validators = Vec::new();
            let mut delegated_puzzle_validators = Vec::new();
            let mut local_delegated_puzzle_wrappers = Vec::new();

            let mut member_validator_solutions = Vec::new();
            let mut delegated_puzzle_validator_solutions = Vec::new();

            for restriction in &self.restrictions {
                match restriction.kind {
                    RestrictionKind::MemberCondition => {
                        let restriction_spend = spend
                            .restrictions
                            .get(&restriction.puzzle_hash)
                            .ok_or(DriverError::MissingSubpathSpend)?;

                        member_validators.push(restriction_spend.puzzle);
                        member_validator_solutions.push(restriction_spend.solution);
                    }
                    RestrictionKind::DelegatedPuzzleHash => {
                        let restriction_spend = spend
                            .restrictions
                            .get(&restriction.puzzle_hash)
                            .ok_or(DriverError::MissingSubpathSpend)?;

                        delegated_puzzle_validators.push(restriction_spend.puzzle);
                        delegated_puzzle_validator_solutions.push(restriction_spend.solution);
                    }
                    RestrictionKind::DelegatedPuzzleWrapper => {
                        local_delegated_puzzle_wrappers.push(restriction.puzzle_hash);
                    }
                }
            }

            for (i, &wrapper) in local_delegated_puzzle_wrappers.iter().enumerate() {
                if i >= delegated_puzzle_wrappers.len() {
                    delegated_puzzle_wrappers.push(wrapper);
                } else if delegated_puzzle_wrappers[i] != wrapper {
                    return Err(DriverError::DelegatedPuzzleWrapperConflict);
                }
            }

            if !local_delegated_puzzle_wrappers.is_empty() {
                delegated_puzzle_validators.push(ctx.curry(
                    EnforceDelegatedPuzzleWrappers::new(&local_delegated_puzzle_wrappers),
                )?);

                delegated_puzzle_validator_solutions.push(ctx.alloc(
                    &EnforceDelegatedPuzzleWrappersSolution::new(
                        ctx.tree_hash(spend.delegated.puzzle).into(),
                    ),
                )?);
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

            let delegated_puzzle_wrappers = delegated_puzzle_wrappers.clone();

            let mut delegated_spend = spend.delegated;

            for wrapper in delegated_puzzle_wrappers.into_iter().rev() {
                let spend = spend
                    .restrictions
                    .get(&wrapper)
                    .ok_or(DriverError::MissingSubpathSpend)?;

                let puzzle = ctx.curry(AddDelegatedPuzzleWrapper::new(
                    spend.puzzle,
                    delegated_spend.puzzle,
                ))?;
                let solution = ctx.alloc(&AddDelegatedPuzzleWrapperSolution::new(
                    spend.solution,
                    delegated_spend.solution,
                ))?;

                delegated_spend = Spend::new(puzzle, solution);
            }

            result.solution = ctx.alloc(&DelegatedFeederSolution::new(
                delegated_spend.puzzle,
                delegated_spend.solution,
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
        let mut delegated_puzzle_wrappers = Vec::new();

        for restriction in restrictions {
            match restriction.kind {
                RestrictionKind::MemberCondition => {
                    member_validators.push(restriction.puzzle_hash);
                }
                RestrictionKind::DelegatedPuzzleHash => {
                    delegated_puzzle_validators.push(restriction.puzzle_hash);
                }
                RestrictionKind::DelegatedPuzzleWrapper => {
                    delegated_puzzle_wrappers.push(restriction.puzzle_hash);
                }
            }
        }

        if !delegated_puzzle_wrappers.is_empty() {
            delegated_puzzle_validators.push(
                EnforceDelegatedPuzzleWrappers::new(&delegated_puzzle_wrappers).curry_tree_hash(),
            );
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
