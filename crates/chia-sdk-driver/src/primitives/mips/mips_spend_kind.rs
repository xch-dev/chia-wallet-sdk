use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::{m_of_n::MofN, mips_spend::MipsSpend};

/// What kind of puzzle is inside of a MIPS inner puzzle's wrappers.
#[derive(Debug, Clone)]
pub enum MipsSpendKind {
    /// A member puzzle (such as a [`BlsMember`](chia_sdk_types::puzzles::BlsMember)), along with its solution.
    Member(Spend),
    /// An M of N puzzle, whose selected items are spent using the members of the [`MipsSpend`].
    MofN(MofN),
}

impl MipsSpendKind {
    /// Constructs the spend of the puzzle, before the MIPS wrappers are applied.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<Spend, DriverError> {
        match self {
            Self::Member(spend) => Ok(*spend),
            Self::MofN(m_of_n) => m_of_n.spend(ctx, spend, delegated_puzzle_wrappers),
        }
    }

    /// Merges the delegated puzzle wrappers of the members that will be spent beneath this puzzle into the stack.
    pub fn collect_delegated_puzzle_wrappers(
        &self,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<(), DriverError> {
        match self {
            Self::Member(_) => Ok(()),
            Self::MofN(m_of_n) => {
                m_of_n.collect_delegated_puzzle_wrappers(spend, delegated_puzzle_wrappers)
            }
        }
    }
}
