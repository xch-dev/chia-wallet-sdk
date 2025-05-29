#![allow(unused)]

use bindy::Result;
use chia_bls::PublicKey;
use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::Bytes32;
use chia_sdk_driver as sdk;

use crate::{Clvm, K1PublicKey, Program, R1PublicKey};

#[derive(Clone)]
pub struct MipsMemo {
    pub inner_puzzle: InnerPuzzleMemo,
}

impl From<MipsMemo> for sdk::MipsMemo {
    fn from(value: MipsMemo) -> Self {
        Self::new(value.inner_puzzle.into())
    }
}

#[derive(Clone)]
pub struct InnerPuzzleMemo {
    pub nonce: u32,
    pub restrictions: Vec<RestrictionMemo>,
    pub kind: MemoKind,
}

impl From<InnerPuzzleMemo> for sdk::InnerPuzzleMemo {
    fn from(value: InnerPuzzleMemo) -> Self {
        Self::new(
            value.nonce.try_into().unwrap(),
            value.restrictions.into_iter().map(Into::into).collect(),
            value.kind.into(),
        )
    }
}

#[derive(Clone)]
pub struct RestrictionMemo {
    pub member_condition_validator: bool,
    pub puzzle_hash: Bytes32,
    pub memo: Program,
}

impl RestrictionMemo {
    pub fn force_1_of_2_restricted_variable(
        clvm: Clvm,
        left_side_subtree_hash: Bytes32,
        nonce: u32,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let restriction = sdk::RestrictionMemo::force_1_of_2_restricted_variable(
            &mut ctx,
            left_side_subtree_hash,
            nonce.try_into().unwrap(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        )?;
        Ok(Self {
            member_condition_validator: restriction.member_condition_validator,
            puzzle_hash: restriction.puzzle_hash,
            memo: Program(clvm.0.clone(), restriction.memo),
        })
    }

    pub fn enforce_delegated_puzzle_wrappers(
        clvm: Clvm,
        wrapper_memos: Vec<WrapperMemo>,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let restriction = sdk::RestrictionMemo::enforce_delegated_puzzle_wrappers(
            &mut ctx,
            &wrapper_memos
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        )?;
        Ok(Self {
            member_condition_validator: restriction.member_condition_validator,
            puzzle_hash: restriction.puzzle_hash,
            memo: Program(clvm.0.clone(), restriction.memo),
        })
    }

    pub fn timelock(clvm: Clvm, seconds: u64, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let restriction = sdk::RestrictionMemo::timelock(&mut ctx, seconds, reveal)?;
        Ok(Self {
            member_condition_validator: restriction.member_condition_validator,
            puzzle_hash: restriction.puzzle_hash,
            memo: Program(clvm.0.clone(), restriction.memo),
        })
    }
}

impl From<RestrictionMemo> for sdk::RestrictionMemo {
    fn from(value: RestrictionMemo) -> Self {
        Self::new(
            value.member_condition_validator,
            value.puzzle_hash,
            value.memo.1,
        )
    }
}

#[derive(Clone)]
pub struct WrapperMemo {
    pub puzzle_hash: Bytes32,
    pub memo: Program,
}

impl WrapperMemo {
    pub fn prevent_vault_side_effects(clvm: Clvm, reveal: bool) -> Result<Vec<Self>> {
        Ok(vec![
            Self::prevent_condition_opcode(clvm.clone(), CREATE_COIN_ANNOUNCEMENT, reveal)?,
            Self::prevent_condition_opcode(clvm.clone(), CREATE_PUZZLE_ANNOUNCEMENT, reveal)?,
            Self::prevent_condition_opcode(clvm.clone(), SEND_MESSAGE, reveal)?,
            Self::prevent_condition_opcode(clvm.clone(), RECEIVE_MESSAGE, reveal)?,
            Self::prevent_multiple_create_coins(clvm)?,
        ])
    }

    pub fn force_coin_announcement(clvm: Clvm) -> Result<Self> {
        let wrapper = sdk::WrapperMemo::force_coin_message();
        Ok(Self {
            puzzle_hash: wrapper.puzzle_hash,
            memo: Program(clvm.0.clone(), wrapper.memo),
        })
    }

    pub fn force_coin_message(clvm: Clvm) -> Result<Self> {
        let wrapper = sdk::WrapperMemo::force_coin_message();
        Ok(Self {
            puzzle_hash: wrapper.puzzle_hash,
            memo: Program(clvm.0.clone(), wrapper.memo),
        })
    }

    pub fn prevent_multiple_create_coins(clvm: Clvm) -> Result<Self> {
        let wrapper = sdk::WrapperMemo::prevent_multiple_create_coins();
        Ok(Self {
            puzzle_hash: wrapper.puzzle_hash,
            memo: Program(clvm.0.clone(), wrapper.memo),
        })
    }

    pub fn timelock(clvm: Clvm, seconds: u64, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let wrapper = sdk::WrapperMemo::timelock(&mut ctx, seconds, reveal)?;
        Ok(Self {
            puzzle_hash: wrapper.puzzle_hash,
            memo: Program(clvm.0.clone(), wrapper.memo),
        })
    }

    pub fn prevent_condition_opcode(clvm: Clvm, opcode: u16, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let wrapper = sdk::WrapperMemo::prevent_condition_opcode(&mut ctx, opcode, reveal)?;
        Ok(Self {
            puzzle_hash: wrapper.puzzle_hash,
            memo: Program(clvm.0.clone(), wrapper.memo),
        })
    }
}

impl From<WrapperMemo> for sdk::WrapperMemo {
    fn from(value: WrapperMemo) -> Self {
        Self::new(value.puzzle_hash, value.memo.1)
    }
}

#[derive(Clone)]
pub struct Force1of2RestrictedVariableMemo {
    pub left_side_subtree_hash: Bytes32,
    pub nonce: u32,
    pub member_validator_list_hash: Bytes32,
    pub delegated_puzzle_validator_list_hash: Bytes32,
}

impl From<Force1of2RestrictedVariableMemo> for sdk::Force1of2RestrictedVariableMemo {
    fn from(value: Force1of2RestrictedVariableMemo) -> Self {
        Self::new(
            value.left_side_subtree_hash,
            value.nonce.try_into().unwrap(),
            value.member_validator_list_hash,
            value.delegated_puzzle_validator_list_hash,
        )
    }
}
#[derive(Clone)]
pub enum MemoKind {
    Member(MemberMemo),
    MofN(MofNMemo),
}

impl MemoKind {
    pub fn member(member: MemberMemo) -> Result<Self> {
        Ok(Self::Member(member))
    }

    pub fn m_of_n(m_of_n: MofNMemo) -> Result<Self> {
        Ok(Self::MofN(m_of_n))
    }

    pub fn as_member(&self) -> Result<Option<MemberMemo>> {
        if let Self::Member(member) = self {
            Ok(Some(member.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn as_m_of_n(&self) -> Result<Option<MofNMemo>> {
        if let Self::MofN(m_of_n) = self {
            Ok(Some(m_of_n.clone()))
        } else {
            Ok(None)
        }
    }
}

impl From<MemoKind> for sdk::MemoKind {
    fn from(value: MemoKind) -> Self {
        match value {
            MemoKind::Member(member) => sdk::MemoKind::Member(member.into()),
            MemoKind::MofN(m_of_n) => sdk::MemoKind::MofN(m_of_n.into()),
        }
    }
}

#[derive(Clone)]
pub struct MemberMemo {
    pub puzzle_hash: Bytes32,
    pub memo: Program,
}

impl MemberMemo {
    pub fn k1(
        clvm: Clvm,
        public_key: K1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::k1(&mut ctx, public_key.0, fast_forward, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }

    pub fn r1(
        clvm: Clvm,
        public_key: R1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::r1(&mut ctx, public_key.0, fast_forward, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }

    pub fn bls(clvm: Clvm, public_key: PublicKey, taproot: bool, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::bls(&mut ctx, public_key, taproot, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }

    pub fn passkey(
        clvm: Clvm,
        public_key: R1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::passkey(&mut ctx, public_key.0, fast_forward, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }

    pub fn singleton(clvm: Clvm, launcher_id: Bytes32, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::singleton(&mut ctx, launcher_id, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }

    pub fn fixed_puzzle(clvm: Clvm, puzzle_hash: Bytes32, reveal: bool) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::fixed_puzzle(&mut ctx, puzzle_hash, reveal)?;
        Ok(Self {
            puzzle_hash: memo.puzzle_hash,
            memo: Program(clvm.0.clone(), memo.memo),
        })
    }
}

impl From<MemberMemo> for sdk::MemberMemo {
    fn from(value: MemberMemo) -> Self {
        Self::new(value.puzzle_hash, value.memo.1)
    }
}

#[derive(Clone)]
pub struct MofNMemo {
    pub required: u32,
    pub items: Vec<InnerPuzzleMemo>,
}

impl From<MofNMemo> for sdk::MofNMemo {
    fn from(value: MofNMemo) -> Self {
        Self::new(
            value.required.try_into().unwrap(),
            value.items.into_iter().map(Into::into).collect(),
        )
    }
}
