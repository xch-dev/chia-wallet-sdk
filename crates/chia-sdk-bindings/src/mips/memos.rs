#![allow(unused)]

use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::PublicKey;
use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::Bytes32;
use chia_sdk_driver::{self as sdk, DriverError, SpendContext};
use clvm_traits::FromClvm;
use clvm_utils::TreeHash;

use crate::{Clvm, K1PublicKey, Program, R1PublicKey};

type SharedContext = Arc<Mutex<SpendContext>>;

fn to_u32(value: usize) -> Result<u32> {
    Ok(u32::try_from(value).map_err(DriverError::from)?)
}

#[derive(Clone)]
pub struct MipsMemo {
    pub inner_puzzle: InnerPuzzleMemo,
}

impl From<MipsMemo> for sdk::MipsMemo {
    fn from(value: MipsMemo) -> Self {
        Self::new(value.inner_puzzle.into())
    }
}

impl MipsMemo {
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::MipsMemo) -> Result<Self> {
        Ok(Self {
            inner_puzzle: InnerPuzzleMemo::from_sdk(clvm, value.inner_puzzle)?,
        })
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(sdk::MipsMemo::from(self.clone()).inner_puzzle_hash())
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

impl InnerPuzzleMemo {
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::InnerPuzzleMemo) -> Result<Self> {
        Ok(Self {
            nonce: to_u32(value.nonce)?,
            restrictions: value
                .restrictions
                .into_iter()
                .map(|restriction| RestrictionMemo::from_sdk(clvm, restriction))
                .collect::<Result<_>>()?,
            kind: MemoKind::from_sdk(clvm, value.kind)?,
        })
    }

    pub fn inner_puzzle_hash(&self, top_level: bool) -> Result<TreeHash> {
        Ok(sdk::InnerPuzzleMemo::from(self.clone()).inner_puzzle_hash(top_level))
    }
}

#[derive(Clone)]
pub struct RestrictionMemo {
    pub member_condition_validator: bool,
    pub puzzle_hash: Bytes32,
    pub memo: Program,
}

impl RestrictionMemo {
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::RestrictionMemo) -> Result<Self> {
        Ok(Self {
            member_condition_validator: value.member_condition_validator,
            puzzle_hash: value.puzzle_hash,
            memo: Program(clvm.clone(), value.memo),
        })
    }

    pub fn parse(&self, ctx: MipsMemoContext) -> Result<Option<ParsedRestriction>> {
        let clvm = &self.memo.0;

        let allocator = clvm.lock().unwrap();
        let ctx = ctx.0.lock().unwrap();

        let Some(parsed) = sdk::RestrictionMemo::from(self.clone()).parse(&allocator, &ctx) else {
            return Ok(None);
        };

        Ok(Some(match parsed {
            sdk::ParsedRestriction::Force1of2RestrictedVariable(..) => {
                let memo =
                    sdk::Force1of2RestrictedVariableMemo::from_clvm(&**allocator, self.memo.1)
                        .map_err(DriverError::from)?;
                ParsedRestriction::Force1of2RestrictedVariable(
                    Force1of2RestrictedVariableMemo::from_sdk(clvm, memo)?,
                )
            }
            sdk::ParsedRestriction::EnforceDelegatedPuzzleWrappers(_, wrappers) => {
                ParsedRestriction::EnforceDelegatedPuzzleWrappers(
                    wrappers
                        .into_iter()
                        .map(|wrapper| WrapperMemo::from_sdk(clvm, wrapper))
                        .collect(),
                )
            }
            sdk::ParsedRestriction::Timelock(timelock) => {
                ParsedRestriction::Timelock(timelock.seconds)
            }
        }))
    }

    pub fn force_1_of_2_restricted_variable(
        clvm: Clvm,
        left_side_subtree_hash: Bytes32,
        nonce: u32,
        restrictions: Vec<RestrictionMemo>,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let restriction = sdk::RestrictionMemo::force_1_of_2_restricted_variable(
            &mut ctx,
            left_side_subtree_hash,
            nonce.try_into().unwrap(),
            restrictions.into_iter().map(Into::into).collect(),
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
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::WrapperMemo) -> Self {
        Self {
            puzzle_hash: value.puzzle_hash,
            memo: Program(clvm.clone(), value.memo),
        }
    }

    pub fn parse(&self, ctx: MipsMemoContext) -> Result<Option<ParsedWrapper>> {
        let clvm = &self.memo.0;

        let allocator = clvm.lock().unwrap();
        let ctx = ctx.0.lock().unwrap();

        let Some(parsed) = sdk::WrapperMemo::from(self.clone()).parse(&allocator, &ctx) else {
            return Ok(None);
        };

        Ok(Some(match parsed {
            sdk::ParsedWrapper::ForceAssertCoinAnnouncement => ParsedWrapper::ForceCoinAnnouncement,
            sdk::ParsedWrapper::ForceCoinMessage => ParsedWrapper::ForceCoinMessage,
            sdk::ParsedWrapper::ForceSingletonRecreation => ParsedWrapper::ForceSingletonRecreation,
            sdk::ParsedWrapper::PreventConditionOpcode(wrapper) => {
                ParsedWrapper::PreventConditionOpcode(wrapper.condition_opcode)
            }
            sdk::ParsedWrapper::PreventMultipleCreateCoins => {
                ParsedWrapper::PreventMultipleCreateCoins
            }
            sdk::ParsedWrapper::Timelock(wrapper) => ParsedWrapper::Timelock(wrapper.seconds),
            sdk::ParsedWrapper::Force1of2RestrictedVariable(..) => {
                let memo =
                    sdk::Force1of2RestrictedVariableMemo::from_clvm(&**allocator, self.memo.1)
                        .map_err(DriverError::from)?;
                ParsedWrapper::Force1of2RestrictedVariable(
                    Force1of2RestrictedVariableMemo::from_sdk(clvm, memo)?,
                )
            }
        }))
    }

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
        let wrapper = sdk::WrapperMemo::force_assert_coin_announcement();
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

    pub fn force_singleton_recreation(clvm: Clvm) -> Result<Self> {
        let wrapper = sdk::WrapperMemo::force_singleton_recreation();
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
    pub restrictions: Vec<RestrictionMemo>,
}

impl Force1of2RestrictedVariableMemo {
    pub(crate) fn from_sdk(
        clvm: &SharedContext,
        value: sdk::Force1of2RestrictedVariableMemo,
    ) -> Result<Self> {
        Ok(Self {
            left_side_subtree_hash: value.left_side_subtree_hash,
            nonce: to_u32(value.nonce)?,
            restrictions: value
                .restrictions
                .into_iter()
                .map(|restriction| RestrictionMemo::from_sdk(clvm, restriction))
                .collect::<Result<_>>()?,
        })
    }
}

impl From<Force1of2RestrictedVariableMemo> for sdk::Force1of2RestrictedVariableMemo {
    fn from(value: Force1of2RestrictedVariableMemo) -> Self {
        Self::new(
            value.left_side_subtree_hash,
            value.nonce.try_into().unwrap(),
            value.restrictions.into_iter().map(Into::into).collect(),
        )
    }
}
#[derive(Clone)]
pub enum MemoKind {
    Member(MemberMemo),
    MofN(MofNMemo),
}

impl MemoKind {
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::MemoKind) -> Result<Self> {
        Ok(match value {
            sdk::MemoKind::Member(member) => Self::Member(MemberMemo::from_sdk(clvm, member)),
            sdk::MemoKind::MofN(m_of_n) => Self::MofN(MofNMemo::from_sdk(clvm, m_of_n)?),
        })
    }

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

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(sdk::MemoKind::from(self.clone()).inner_puzzle_hash())
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
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::MemberMemo) -> Self {
        Self {
            puzzle_hash: value.puzzle_hash,
            memo: Program(clvm.clone(), value.memo),
        }
    }

    pub fn parse(&self, ctx: MipsMemoContext) -> Result<Option<ParsedMember>> {
        let parsed = {
            let allocator = self.memo.0.lock().unwrap();
            let ctx = ctx.0.lock().unwrap();
            sdk::MemberMemo::from(self.clone()).parse(&allocator, &ctx)
        };

        Ok(parsed.map(|parsed| ParsedMember::from_sdk(&self.memo.0, parsed)))
    }

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

    pub fn bls(
        clvm: Clvm,
        public_key: PublicKey,
        fast_forward: bool,
        taproot: bool,
        reveal: bool,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::bls(&mut ctx, public_key, fast_forward, taproot, reveal)?;
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

    pub fn singleton(
        clvm: Clvm,
        launcher_id: Bytes32,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let memo = sdk::MemberMemo::singleton(&mut ctx, launcher_id, fast_forward, reveal)?;
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

impl MofNMemo {
    pub(crate) fn from_sdk(clvm: &SharedContext, value: sdk::MofNMemo) -> Result<Self> {
        Ok(Self {
            required: to_u32(value.required)?,
            items: value
                .items
                .into_iter()
                .map(|item| InnerPuzzleMemo::from_sdk(clvm, item))
                .collect::<Result<_>>()?,
        })
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(sdk::MofNMemo::from(self.clone()).inner_puzzle_hash())
    }
}

#[derive(Clone)]
pub struct MipsMemoContext(Arc<Mutex<sdk::MipsMemoContext>>);

impl MipsMemoContext {
    pub fn new() -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(sdk::MipsMemoContext::default()))))
    }

    pub fn add_k1(&self, public_key: K1PublicKey) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.k1.push(public_key.0);
        Ok(())
    }

    pub fn add_r1(&self, public_key: R1PublicKey) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.r1.push(public_key.0);
        Ok(())
    }

    pub fn add_bls(&self, public_key: PublicKey) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.bls.push(public_key);
        Ok(())
    }

    pub fn add_hash(&self, hash: Bytes32) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.hashes.push(hash);
        Ok(())
    }

    pub fn add_timelock(&self, timelock: u64) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.timelocks.push(timelock);
        Ok(())
    }

    pub fn add_opcode(&self, opcode: u16) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.opcodes.push(opcode);
        Ok(())
    }

    pub fn add_singleton_mode(&self, mode: u8) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        ctx.singleton_modes.push(mode);
        Ok(())
    }
}

#[derive(Clone)]
pub enum ParsedMember {
    K1 {
        public_key: K1PublicKey,
        fast_forward: bool,
    },
    R1 {
        public_key: R1PublicKey,
        fast_forward: bool,
    },
    Bls {
        public_key: PublicKey,
        fast_forward: bool,
    },
    BlsTaproot {
        synthetic_key: PublicKey,
        fast_forward: bool,
    },
    Passkey {
        public_key: R1PublicKey,
        fast_forward: bool,
    },
    Singleton {
        launcher_id: Bytes32,
        mode: Option<u8>,
    },
    FixedPuzzle(Bytes32),
    Custom(Program),
}

impl ParsedMember {
    fn from_sdk(clvm: &SharedContext, value: sdk::ParsedMember) -> Self {
        match value {
            sdk::ParsedMember::K1(member) => Self::K1 {
                public_key: K1PublicKey(member.public_key),
                fast_forward: false,
            },
            sdk::ParsedMember::K1PuzzleAssert(member) => Self::K1 {
                public_key: K1PublicKey(member.public_key),
                fast_forward: true,
            },
            sdk::ParsedMember::R1(member) => Self::R1 {
                public_key: R1PublicKey(member.public_key),
                fast_forward: false,
            },
            sdk::ParsedMember::R1PuzzleAssert(member) => Self::R1 {
                public_key: R1PublicKey(member.public_key),
                fast_forward: true,
            },
            sdk::ParsedMember::Bls(member) => Self::Bls {
                public_key: member.public_key,
                fast_forward: false,
            },
            sdk::ParsedMember::BlsPuzzleAssert(member) => Self::Bls {
                public_key: member.public_key,
                fast_forward: true,
            },
            sdk::ParsedMember::BlsTaproot(member) => Self::BlsTaproot {
                synthetic_key: member.synthetic_key,
                fast_forward: false,
            },
            sdk::ParsedMember::BlsTaprootPuzzleAssert(member) => Self::BlsTaproot {
                synthetic_key: member.synthetic_key,
                fast_forward: true,
            },
            sdk::ParsedMember::Passkey(member) => Self::Passkey {
                public_key: R1PublicKey(member.public_key),
                fast_forward: false,
            },
            sdk::ParsedMember::PasskeyPuzzleAssert(member) => Self::Passkey {
                public_key: R1PublicKey(member.public_key),
                fast_forward: true,
            },
            sdk::ParsedMember::Singleton(member) => Self::Singleton {
                launcher_id: member.singleton_struct.launcher_id,
                mode: None,
            },
            sdk::ParsedMember::SingletonWithMode(member) => Self::Singleton {
                launcher_id: member.singleton_struct.launcher_id,
                mode: Some(member.mode),
            },
            sdk::ParsedMember::FixedPuzzle(member) => Self::FixedPuzzle(member.fixed_puzzle_hash),
            sdk::ParsedMember::Custom(puzzle) => Self::Custom(Program(clvm.clone(), puzzle)),
        }
    }

    pub fn as_k1(&self) -> Result<Option<K1PublicKey>> {
        match self {
            Self::K1 { public_key, .. } => Ok(Some(*public_key)),
            _ => Ok(None),
        }
    }

    pub fn as_r1(&self) -> Result<Option<R1PublicKey>> {
        match self {
            Self::R1 { public_key, .. } => Ok(Some(*public_key)),
            _ => Ok(None),
        }
    }

    pub fn as_bls(&self) -> Result<Option<PublicKey>> {
        match self {
            Self::Bls { public_key, .. } => Ok(Some(*public_key)),
            _ => Ok(None),
        }
    }

    pub fn as_bls_taproot(&self) -> Result<Option<PublicKey>> {
        match self {
            Self::BlsTaproot { synthetic_key, .. } => Ok(Some(*synthetic_key)),
            _ => Ok(None),
        }
    }

    pub fn as_passkey(&self) -> Result<Option<R1PublicKey>> {
        match self {
            Self::Passkey { public_key, .. } => Ok(Some(*public_key)),
            _ => Ok(None),
        }
    }

    pub fn as_singleton(&self) -> Result<Option<Bytes32>> {
        match self {
            Self::Singleton { launcher_id, .. } => Ok(Some(*launcher_id)),
            _ => Ok(None),
        }
    }

    pub fn singleton_mode(&self) -> Result<Option<u8>> {
        match self {
            Self::Singleton { mode, .. } => Ok(*mode),
            _ => Ok(None),
        }
    }

    pub fn as_fixed_puzzle(&self) -> Result<Option<Bytes32>> {
        match self {
            Self::FixedPuzzle(puzzle_hash) => Ok(Some(*puzzle_hash)),
            _ => Ok(None),
        }
    }

    pub fn as_custom(&self) -> Result<Option<Program>> {
        match self {
            Self::Custom(puzzle) => Ok(Some(puzzle.clone())),
            _ => Ok(None),
        }
    }

    pub fn fast_forward(&self) -> Result<bool> {
        Ok(match self {
            Self::K1 { fast_forward, .. }
            | Self::R1 { fast_forward, .. }
            | Self::Bls { fast_forward, .. }
            | Self::BlsTaproot { fast_forward, .. }
            | Self::Passkey { fast_forward, .. } => *fast_forward,
            // Mode 0b010_010 is the one used when constructing a fast forward singleton member.
            Self::Singleton { mode, .. } => *mode == Some(0b010_010),
            Self::FixedPuzzle(_) | Self::Custom(_) => false,
        })
    }
}

#[derive(Clone)]
pub enum ParsedRestriction {
    Force1of2RestrictedVariable(Force1of2RestrictedVariableMemo),
    EnforceDelegatedPuzzleWrappers(Vec<WrapperMemo>),
    Timelock(u64),
}

impl ParsedRestriction {
    pub fn as_force_1_of_2_restricted_variable(
        &self,
    ) -> Result<Option<Force1of2RestrictedVariableMemo>> {
        match self {
            Self::Force1of2RestrictedVariable(memo) => Ok(Some(memo.clone())),
            _ => Ok(None),
        }
    }

    pub fn as_enforce_delegated_puzzle_wrappers(&self) -> Result<Option<Vec<WrapperMemo>>> {
        match self {
            Self::EnforceDelegatedPuzzleWrappers(wrappers) => Ok(Some(wrappers.clone())),
            _ => Ok(None),
        }
    }

    pub fn as_timelock(&self) -> Result<Option<u64>> {
        match self {
            Self::Timelock(seconds) => Ok(Some(*seconds)),
            _ => Ok(None),
        }
    }
}

#[derive(Clone)]
pub enum ParsedWrapper {
    ForceCoinAnnouncement,
    ForceCoinMessage,
    ForceSingletonRecreation,
    PreventConditionOpcode(u16),
    PreventMultipleCreateCoins,
    Timelock(u64),
    Force1of2RestrictedVariable(Force1of2RestrictedVariableMemo),
}

impl ParsedWrapper {
    pub fn is_force_coin_announcement(&self) -> Result<bool> {
        Ok(matches!(self, Self::ForceCoinAnnouncement))
    }

    pub fn is_force_coin_message(&self) -> Result<bool> {
        Ok(matches!(self, Self::ForceCoinMessage))
    }

    pub fn is_force_singleton_recreation(&self) -> Result<bool> {
        Ok(matches!(self, Self::ForceSingletonRecreation))
    }

    pub fn as_prevent_condition_opcode(&self) -> Result<Option<u16>> {
        match self {
            Self::PreventConditionOpcode(opcode) => Ok(Some(*opcode)),
            _ => Ok(None),
        }
    }

    pub fn is_prevent_multiple_create_coins(&self) -> Result<bool> {
        Ok(matches!(self, Self::PreventMultipleCreateCoins))
    }

    pub fn as_timelock(&self) -> Result<Option<u64>> {
        match self {
            Self::Timelock(seconds) => Ok(Some(*seconds)),
            _ => Ok(None),
        }
    }

    pub fn as_force_1_of_2_restricted_variable(
        &self,
    ) -> Result<Option<Force1of2RestrictedVariableMemo>> {
        match self {
            Self::Force1of2RestrictedVariable(memo) => Ok(Some(memo.clone())),
            _ => Ok(None),
        }
    }
}
