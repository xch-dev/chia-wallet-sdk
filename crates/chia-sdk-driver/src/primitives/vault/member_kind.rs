use crate::{DriverError, Spend, SpendContext};

use super::{m_of_n::MofN, vault_spend::VaultSpend};

#[derive(Debug, Clone)]
pub enum MemberSpendKind {
    Leaf(Spend),
    MofN(MofN),
}

impl MemberSpendKind {
    pub fn spend(&self, ctx: &mut SpendContext, spend: &VaultSpend) -> Result<Spend, DriverError> {
        match self {
            Self::Leaf(spend) => Ok(*spend),
            Self::MofN(m_of_n) => m_of_n.spend(ctx, spend),
        }
    }
}
