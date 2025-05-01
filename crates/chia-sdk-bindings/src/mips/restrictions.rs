use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver as sdk;
use chia_sdk_types::{
    puzzles::{
        Force1of2RestrictedVariable, PreventConditionOpcode, Timelock,
        PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
    },
    Mod,
};
use clvm_utils::TreeHash;

#[derive(Clone)]
pub struct Restriction {
    pub kind: RestrictionKind,
    pub puzzle_hash: TreeHash,
}

impl From<Restriction> for sdk::Restriction {
    fn from(value: Restriction) -> Self {
        sdk::Restriction {
            kind: value.kind.into(),
            puzzle_hash: value.puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub enum RestrictionKind {
    MemberCondition,
    DelegatedPuzzleHash,
    DelegatedPuzzleWrapper,
}

impl From<RestrictionKind> for sdk::RestrictionKind {
    fn from(value: RestrictionKind) -> Self {
        match value {
            RestrictionKind::MemberCondition => sdk::RestrictionKind::MemberCondition,
            RestrictionKind::DelegatedPuzzleHash => sdk::RestrictionKind::DelegatedPuzzleHash,
            RestrictionKind::DelegatedPuzzleWrapper => sdk::RestrictionKind::DelegatedPuzzleWrapper,
        }
    }
}

pub(crate) fn convert_restrictions(restrictions: Vec<Restriction>) -> Vec<sdk::Restriction> {
    restrictions.into_iter().map(Into::into).collect()
}

pub fn timelock_restriction(timelock: u64) -> Result<Restriction> {
    Ok(Restriction {
        kind: RestrictionKind::MemberCondition,
        puzzle_hash: Timelock::new(timelock).curry_tree_hash(),
    })
}

pub fn force_1_of_2_restriction(
    left_side_subtree_hash: Bytes32,
    nonce: u32,
    member_validator_list_hash: Bytes32,
    delegated_puzzle_validator_list_hash: Bytes32,
) -> Result<Restriction> {
    Ok(Restriction {
        kind: RestrictionKind::DelegatedPuzzleWrapper,
        puzzle_hash: Force1of2RestrictedVariable::new(
            left_side_subtree_hash,
            nonce.try_into().unwrap(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        )
        .curry_tree_hash(),
    })
}

pub fn prevent_condition_opcode_restriction(condition_opcode: u16) -> Result<Restriction> {
    Ok(Restriction {
        kind: RestrictionKind::DelegatedPuzzleWrapper,
        puzzle_hash: PreventConditionOpcode::new(condition_opcode).curry_tree_hash(),
    })
}

pub fn prevent_multiple_create_coins_restriction() -> Result<Restriction> {
    Ok(Restriction {
        kind: RestrictionKind::DelegatedPuzzleWrapper,
        puzzle_hash: PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
    })
}

pub fn prevent_vault_side_effects_restriction() -> Result<Vec<Restriction>> {
    Ok(vec![
        prevent_condition_opcode_restriction(60)?,
        prevent_condition_opcode_restriction(62)?,
        prevent_condition_opcode_restriction(66)?,
        prevent_condition_opcode_restriction(67)?,
        prevent_multiple_create_coins_restriction()?,
    ])
}
