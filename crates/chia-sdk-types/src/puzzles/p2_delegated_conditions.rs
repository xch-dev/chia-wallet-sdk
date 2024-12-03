use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::NodePtr;
use hex_literal::hex;

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
    const REVEAL: &[u8] = &P2_DELEGATED_CONDITIONS_PUZZLE;
    const HASH: TreeHash = P2_DELEGATED_CONDITIONS_PUZZLE_HASH;
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2DelegatedConditionsSolution<T = NodePtr> {
    pub conditions: Vec<Condition<T>>,
}

impl P2DelegatedConditionsSolution {
    pub fn new(conditions: Vec<Condition>) -> Self {
        Self { conditions }
    }
}

pub const P2_DELEGATED_CONDITIONS_PUZZLE: [u8; 137] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff
    04ff0bff80808080ff80808080ff0b80ffff04ffff01ff32ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080
    ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

pub const P2_DELEGATED_CONDITIONS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "0ff94726f1a8dea5c3f70d3121945190778d3b2b3fcda3735a1f290977e98341"
));
