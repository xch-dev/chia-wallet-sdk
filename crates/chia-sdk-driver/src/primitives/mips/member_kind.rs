use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::{m_of_n::MofN, mips_spend::MipsSpend};

#[derive(Debug, Clone)]
pub enum MemberSpendKind {
    Leaf(Spend),
    MofN(MofN),
}

impl MemberSpendKind {
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        spend: &MipsSpend,
        delegated_puzzle_wrappers: &mut Vec<TreeHash>,
    ) -> Result<Spend, DriverError> {
        match self {
            Self::Leaf(spend) => Ok(*spend),
            Self::MofN(m_of_n) => m_of_n.spend(ctx, spend, delegated_puzzle_wrappers),
        }
    }
}
