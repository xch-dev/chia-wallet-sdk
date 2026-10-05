mod parsed_member;
mod parsed_restriction;
mod parsed_wrapper;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vault_tests;

use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_puzzles::{
    FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH, FORCE_COIN_MESSAGE_HASH,
    PREVENT_MULTIPLE_CREATE_COINS_HASH,
};
pub use parsed_member::*;
pub use parsed_restriction::*;
pub use parsed_wrapper::*;

use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    MerkleTree, Mod,
    puzzles::{
        BlsMember, BlsMemberPuzzleAssert, BlsTaprootMember, BlsTaprootMemberPuzzleAssert,
        DelegatedPuzzleFeederArgs, EnforceDelegatedPuzzleWrappers, FORCE_SINGLETON_RECREATION_HASH,
        FixedPuzzleMember, Force1of2RestrictedVariable, IndexWrapperArgs, K1Member,
        K1MemberPuzzleAssert, MofNArgs, NofNArgs, OneOfNArgs, PasskeyMember,
        PasskeyMemberPuzzleAssert, PreventConditionOpcode, R1Member, R1MemberPuzzleAssert,
        RestrictionsArgs, SingletonMember, SingletonMemberWithMode, Timelock,
    },
};
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_traits::{FromClvm, ToClvm, apply_constants};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct MipsMemo<T = NodePtr> {
    #[clvm(constant = "CHIP-0043".to_string())]
    pub namespace: String,
    pub inner_puzzle: InnerPuzzleMemo<T>,
}

impl MipsMemo<NodePtr> {
    pub fn new(inner_puzzle: InnerPuzzleMemo) -> Self {
        Self { inner_puzzle }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        self.inner_puzzle.inner_puzzle_hash(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InnerPuzzleMemo<T = NodePtr> {
    pub nonce: usize,
    pub restrictions: Vec<RestrictionMemo<T>>,
    #[clvm(rest)]
    pub kind: MemoKind<T>,
}

impl InnerPuzzleMemo<NodePtr> {
    pub fn new(nonce: usize, restrictions: Vec<RestrictionMemo>, kind: MemoKind) -> Self {
        Self {
            nonce,
            restrictions,
            kind,
        }
    }

    pub fn inner_puzzle_hash(&self, top_level: bool) -> TreeHash {
        let mut puzzle_hash = self.kind.inner_puzzle_hash();

        if !self.restrictions.is_empty() {
            let (member_validators, delegated_puzzle_validators) =
                split_restrictions(&self.restrictions);

            puzzle_hash =
                RestrictionsArgs::new(member_validators, delegated_puzzle_validators, puzzle_hash)
                    .curry_tree_hash();
        }

        if top_level {
            puzzle_hash = DelegatedPuzzleFeederArgs::new(puzzle_hash).curry_tree_hash();
        }

        IndexWrapperArgs::new(self.nonce, puzzle_hash).curry_tree_hash()
    }
}

/// Splits restrictions into the member validator and delegated puzzle validator lists curried
/// into the restrictions puzzle, preserving the order within each list.
fn split_restrictions(restrictions: &[RestrictionMemo]) -> (Vec<TreeHash>, Vec<TreeHash>) {
    let mut member_validators = Vec::new();
    let mut delegated_puzzle_validators = Vec::new();

    for restriction in restrictions {
        if restriction.member_condition_validator {
            member_validators.push(restriction.puzzle_hash.into());
        } else {
            delegated_puzzle_validators.push(restriction.puzzle_hash.into());
        }
    }

    (member_validators, delegated_puzzle_validators)
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionMemo<T = NodePtr> {
    pub member_condition_validator: bool,
    pub puzzle_hash: Bytes32,
    pub memo: T,
}

impl RestrictionMemo<NodePtr> {
    pub fn new(member_condition_validator: bool, puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self {
            member_condition_validator,
            puzzle_hash,
            memo,
        }
    }

    /// The restrictions are the ones applied to the new right side once the 1 of 2 is spent.
    pub fn force_1_of_2_restricted_variable(
        allocator: &mut Allocator,
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        restrictions: Vec<RestrictionMemo>,
    ) -> Result<Self, DriverError> {
        let memo =
            Force1of2RestrictedVariableMemo::new(left_side_subtree_hash, nonce, restrictions);

        Ok(Self::new(
            false,
            memo.restriction().curry_tree_hash().into(),
            memo.to_clvm(allocator)?,
        ))
    }

    pub fn enforce_delegated_puzzle_wrappers(
        allocator: &mut Allocator,
        wrapper_memos: &[WrapperMemo],
    ) -> Result<Self, DriverError> {
        let wrapper_stack: Vec<TreeHash> = wrapper_memos
            .iter()
            .map(|item| TreeHash::from(item.puzzle_hash))
            .collect();

        Ok(Self::new(
            false,
            EnforceDelegatedPuzzleWrappers::new(&wrapper_stack)
                .curry_tree_hash()
                .into(),
            wrapper_memos.to_clvm(allocator)?,
        ))
    }

    pub fn timelock(
        allocator: &mut Allocator,
        seconds: u64,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            true,
            Timelock::new(seconds).curry_tree_hash().into(),
            if reveal {
                seconds.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn parse(&self, allocator: &Allocator, ctx: &MipsMemoContext) -> Option<ParsedRestriction> {
        if let Ok(wrappers) = Vec::<WrapperMemo>::from_clvm(allocator, self.memo) {
            let wrapper_stack: Vec<TreeHash> = wrappers
                .iter()
                .map(|wrapper| wrapper.puzzle_hash.into())
                .collect();
            let restriction = EnforceDelegatedPuzzleWrappers::new(&wrapper_stack);

            if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(
                    restriction,
                    wrappers,
                ));
            }
        }

        if let Ok(memo) = Force1of2RestrictedVariableMemo::from_clvm(allocator, self.memo) {
            let restriction = memo.restriction();

            if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedRestriction::Force1of2RestrictedVariable(
                    restriction,
                    memo.restrictions,
                ));
            }
        }

        if let Ok(seconds) = Option::<u64>::from_clvm(allocator, self.memo) {
            for &seconds in seconds.iter().chain(ctx.timelocks.iter()) {
                let restriction = Timelock::new(seconds);

                if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedRestriction::Timelock(restriction));
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct WrapperMemo<T = NodePtr> {
    pub puzzle_hash: Bytes32,
    pub memo: T,
}

impl WrapperMemo<NodePtr> {
    pub fn new(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self { puzzle_hash, memo }
    }

    pub fn force_assert_coin_announcement() -> Self {
        Self {
            puzzle_hash: FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn force_coin_message() -> Self {
        Self {
            puzzle_hash: FORCE_COIN_MESSAGE_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn prevent_multiple_create_coins() -> Self {
        Self {
            puzzle_hash: PREVENT_MULTIPLE_CREATE_COINS_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn force_singleton_recreation() -> Self {
        Self {
            puzzle_hash: FORCE_SINGLETON_RECREATION_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn timelock(
        allocator: &mut Allocator,
        seconds: u64,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self {
            puzzle_hash: Timelock::new(seconds).curry_tree_hash().into(),
            memo: if reveal {
                seconds.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        })
    }

    pub fn prevent_condition_opcode(
        allocator: &mut Allocator,
        opcode: u16,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self {
            puzzle_hash: PreventConditionOpcode::new(opcode).curry_tree_hash().into(),
            memo: if reveal {
                opcode.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        })
    }

    pub fn parse(&self, allocator: &Allocator, ctx: &MipsMemoContext) -> Option<ParsedWrapper> {
        if self.puzzle_hash == FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH.into() {
            return Some(ParsedWrapper::ForceAssertCoinAnnouncement);
        }

        if self.puzzle_hash == FORCE_COIN_MESSAGE_HASH.into() {
            return Some(ParsedWrapper::ForceCoinMessage);
        }

        if self.puzzle_hash == FORCE_SINGLETON_RECREATION_HASH.into() {
            return Some(ParsedWrapper::ForceSingletonRecreation);
        }

        if self.puzzle_hash == PREVENT_MULTIPLE_CREATE_COINS_HASH.into() {
            return Some(ParsedWrapper::PreventMultipleCreateCoins);
        }

        if let Ok(memo) = Force1of2RestrictedVariableMemo::from_clvm(allocator, self.memo) {
            let wrapper = memo.restriction();

            if wrapper.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedWrapper::Force1of2RestrictedVariable(
                    wrapper,
                    memo.restrictions,
                ));
            }
        }

        if let Ok(seconds) = Option::<u64>::from_clvm(allocator, self.memo) {
            for &seconds in seconds.iter().chain(ctx.timelocks.iter()) {
                let wrapper = Timelock::new(seconds);

                if wrapper.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedWrapper::Timelock(wrapper));
                }
            }
        }

        if let Ok(opcode) = Option::<u16>::from_clvm(allocator, self.memo) {
            for &opcode in opcode.iter().chain(ctx.opcodes.iter()) {
                let wrapper = PreventConditionOpcode::new(opcode);

                if wrapper.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedWrapper::PreventConditionOpcode(wrapper));
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Force1of2RestrictedVariableMemo<T = NodePtr> {
    pub left_side_subtree_hash: Bytes32,
    pub nonce: usize,
    pub restrictions: Vec<RestrictionMemo<T>>,
}

impl Force1of2RestrictedVariableMemo<NodePtr> {
    pub fn new(
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        restrictions: Vec<RestrictionMemo>,
    ) -> Self {
        Self {
            left_side_subtree_hash,
            nonce,
            restrictions,
        }
    }

    pub fn restriction(&self) -> Force1of2RestrictedVariable {
        let (member_validators, delegated_puzzle_validators) =
            split_restrictions(&self.restrictions);

        Force1of2RestrictedVariable::new(
            self.left_side_subtree_hash,
            self.nonce,
            member_validators.tree_hash().into(),
            delegated_puzzle_validators.tree_hash().into(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub enum MemoKind<T = NodePtr> {
    Member(MemberMemo<T>),
    MofN(MofNMemo<T>),
}

impl MemoKind<NodePtr> {
    pub fn inner_puzzle_hash(&self) -> TreeHash {
        match self {
            Self::Member(member) => member.puzzle_hash.into(),
            Self::MofN(m_of_n) => m_of_n.inner_puzzle_hash(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MemberMemo<T = NodePtr> {
    pub puzzle_hash: Bytes32,
    pub memo: T,
}

impl MemberMemo<NodePtr> {
    pub fn new(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self { puzzle_hash, memo }
    }

    pub fn k1(
        allocator: &mut Allocator,
        public_key: K1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if fast_forward {
                K1MemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                K1Member::new(public_key).curry_tree_hash()
            }
            .into(),
            if reveal {
                public_key.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn r1(
        allocator: &mut Allocator,
        public_key: R1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if fast_forward {
                R1MemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                R1Member::new(public_key).curry_tree_hash()
            }
            .into(),
            if reveal {
                public_key.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn bls(
        allocator: &mut Allocator,
        public_key: PublicKey,
        fast_forward: bool,
        taproot: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if taproot {
                if fast_forward {
                    BlsTaprootMemberPuzzleAssert::new(public_key)
                        .curry_tree_hash()
                        .into()
                } else {
                    BlsTaprootMember::new(public_key).curry_tree_hash().into()
                }
            } else if fast_forward {
                BlsMemberPuzzleAssert::new(public_key)
                    .curry_tree_hash()
                    .into()
            } else {
                BlsMember::new(public_key).curry_tree_hash().into()
            },
            if reveal {
                public_key.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn passkey(
        allocator: &mut Allocator,
        public_key: R1PublicKey,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if fast_forward {
                PasskeyMemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                PasskeyMember::new(public_key).curry_tree_hash()
            }
            .into(),
            if reveal {
                public_key.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn singleton(
        allocator: &mut Allocator,
        launcher_id: Bytes32,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if fast_forward {
                SingletonMemberWithMode::new(launcher_id, 0b010_010)
                    .curry_tree_hash()
                    .into()
            } else {
                SingletonMember::new(launcher_id).curry_tree_hash().into()
            },
            if reveal {
                launcher_id.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn fixed_puzzle(
        allocator: &mut Allocator,
        puzzle_hash: Bytes32,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            FixedPuzzleMember::new(puzzle_hash).curry_tree_hash().into(),
            if reveal {
                puzzle_hash.to_clvm(allocator)?
            } else {
                NodePtr::NIL
            },
        ))
    }

    pub fn parse(&self, allocator: &Allocator, ctx: &MipsMemoContext) -> Option<ParsedMember> {
        for &public_key in Option::<PublicKey>::from_clvm(allocator, self.memo)
            .ok()
            .flatten()
            .iter()
            .chain(ctx.bls.iter())
        {
            let member = BlsMember::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::Bls(member));
            }

            let member = BlsMemberPuzzleAssert::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::BlsPuzzleAssert(member));
            }

            let member = BlsTaprootMember::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::BlsTaproot(member));
            }

            let member = BlsTaprootMemberPuzzleAssert::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::BlsTaprootPuzzleAssert(member));
            }
        }

        for &public_key in Option::<K1PublicKey>::from_clvm(allocator, self.memo)
            .ok()
            .flatten()
            .iter()
            .chain(ctx.k1.iter())
        {
            let member = K1Member::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::K1(member));
            }

            let member = K1MemberPuzzleAssert::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::K1PuzzleAssert(member));
            }
        }

        for &public_key in Option::<R1PublicKey>::from_clvm(allocator, self.memo)
            .ok()
            .flatten()
            .iter()
            .chain(ctx.r1.iter())
        {
            let member = R1Member::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::R1(member));
            }

            let member = R1MemberPuzzleAssert::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::R1PuzzleAssert(member));
            }

            let member = PasskeyMember::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::Passkey(member));
            }

            let member = PasskeyMemberPuzzleAssert::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::PasskeyPuzzleAssert(member));
            }
        }

        for &hash in Option::<Bytes32>::from_clvm(allocator, self.memo)
            .ok()
            .flatten()
            .iter()
            .chain(ctx.hashes.iter())
        {
            let member = SingletonMember::new(hash);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::Singleton(member));
            }

            for &mode in &ctx.singleton_modes {
                let member = SingletonMemberWithMode::new(hash, mode);
                if member.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedMember::SingletonWithMode(member));
                }
            }

            let member = FixedPuzzleMember::new(hash);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::FixedPuzzle(member));
            }
        }

        if tree_hash(allocator, self.memo) == self.puzzle_hash.into() {
            return Some(ParsedMember::Custom(self.memo));
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNMemo<T = NodePtr> {
    pub required: usize,
    pub items: Vec<InnerPuzzleMemo<T>>,
}

impl MofNMemo<NodePtr> {
    pub fn new(required: usize, items: Vec<InnerPuzzleMemo>) -> Self {
        Self { required, items }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        let leaves = self
            .items
            .iter()
            .map(|member| member.inner_puzzle_hash(false).into())
            .collect::<Vec<_>>();
        let merkle_tree = MerkleTree::new(&leaves);

        if self.required == 1 {
            OneOfNArgs::new(merkle_tree.root()).curry_tree_hash()
        } else if self.required == self.items.len() {
            NofNArgs::new(
                self.items
                    .iter()
                    .map(|member| member.inner_puzzle_hash(false))
                    .collect(),
            )
            .curry_tree_hash()
        } else {
            MofNArgs::new(self.required, merkle_tree.root()).curry_tree_hash()
        }
    }
}

#[derive(Debug, Clone)]
pub struct MipsMemoContext {
    pub k1: Vec<K1PublicKey>,
    pub r1: Vec<R1PublicKey>,
    pub bls: Vec<PublicKey>,
    pub hashes: Vec<Bytes32>,
    pub timelocks: Vec<u64>,
    pub opcodes: Vec<u16>,
    pub singleton_modes: Vec<u8>,
}

impl Default for MipsMemoContext {
    fn default() -> Self {
        Self {
            k1: vec![],
            r1: vec![],
            bls: vec![],
            hashes: vec![],
            timelocks: vec![],
            opcodes: vec![
                SEND_MESSAGE,
                RECEIVE_MESSAGE,
                CREATE_PUZZLE_ANNOUNCEMENT,
                CREATE_COIN_ANNOUNCEMENT,
            ],
            singleton_modes: vec![0b010_010, 0b010_111],
        }
    }
}
