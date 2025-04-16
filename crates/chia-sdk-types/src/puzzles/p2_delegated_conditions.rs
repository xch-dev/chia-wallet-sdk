use chia_bls::PublicKey;
use chia_puzzles::{P2_DELEGATED_CONDITIONS, P2_DELEGATED_CONDITIONS_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{Condition, Mod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2DelegatedConditionsArgs {
    pub public_key: PublicKey,
}

impl P2DelegatedConditionsArgs {
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for P2DelegatedConditionsArgs {
    const MOD_REVEAL: &[u8] = &P2_DELEGATED_CONDITIONS;
    const MOD_HASH: TreeHash = TreeHash::new(P2_DELEGATED_CONDITIONS_HASH);
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2DelegatedConditionsSolution<T = NodePtr> {
    pub conditions: Vec<Condition<T>>,
}

impl P2DelegatedConditionsSolution {
    pub fn new(conditions: Vec<Condition>) -> Self {
        Self { conditions }
    }
}
