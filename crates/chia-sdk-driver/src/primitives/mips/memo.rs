mod parsed_member;
mod parsed_restriction;
mod parsed_wrapper;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vault_tests;

use std::collections::HashSet;

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
    Mod,
    puzzles::{
        BlsMember, BlsMemberPuzzleAssert, BlsTaprootMember, BlsTaprootMemberPuzzleAssert,
        EnforceDelegatedPuzzleWrappers, FORCE_SINGLETON_RECREATION_HASH, FixedPuzzleMember,
        Force1of2RestrictedVariable, K1Member, K1MemberPuzzleAssert, PasskeyMember,
        PasskeyMemberPuzzleAssert, PreventConditionOpcode, R1Member, R1MemberPuzzleAssert,
        SingletonMember, SingletonMemberWithMode, Timelock,
    },
};
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_traits::{FromClvm, ToClvm, apply_constants};
use clvm_utils::{ToTreeHash, TreeCache, TreeHash, tree_hash_cached};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, MofN, wrap_mips_puzzle_hash};

/// The maximum nesting depth of M of N memos accepted by [`MipsMemo::from_clvm_bounded`].
pub const MAX_MIPS_MEMO_DEPTH: usize = 32;

/// The maximum number of inner puzzle and restriction memos accepted by
/// [`MipsMemo::from_clvm_bounded`]. Shared subtrees are counted once per occurrence.
pub const MAX_MIPS_MEMO_NODES: usize = 10_000;

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

    /// Like [`MipsMemo::inner_puzzle_hash`], but fails if the memo describes an M of N that can't
    /// be spent as described (see [`MofNMemo::try_inner_puzzle_hash`]).
    pub fn try_inner_puzzle_hash(&self) -> Result<TreeHash, DriverError> {
        self.inner_puzzle.try_inner_puzzle_hash(true)
    }

    /// Returns `true` if the memo is well formed and describes the given inner puzzle hash.
    pub fn verify(&self, expected_inner_puzzle_hash: TreeHash) -> bool {
        self.try_inner_puzzle_hash()
            .is_ok_and(|hash| hash == expected_inner_puzzle_hash)
    }

    /// Parses a memo from untrusted input. Unlike [`FromClvm`], this rejects memos which are nested
    /// too deeply or which expand to too many nodes (for example by sharing subtrees), as well as
    /// memos which fail [`MipsMemo::try_inner_puzzle_hash`].
    pub fn from_clvm_bounded(allocator: &Allocator, node: NodePtr) -> Result<Self, DriverError> {
        let shallow = ShallowMipsMemo::<NodePtr>::from_clvm(allocator, node)?;
        check_inner_puzzle_memo_bounds(allocator, shallow.inner_puzzle, 0, &mut 0)?;

        let memo = Self::from_clvm(allocator, node)?;
        memo.try_inner_puzzle_hash()?;

        Ok(memo)
    }
}

/// Mirrors the encoding of [`MipsMemo`] without decoding the inner puzzle.
#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[clvm(list)]
struct ShallowMipsMemo<T> {
    #[clvm(constant = "CHIP-0043".to_string())]
    namespace: String,
    inner_puzzle: T,
}

/// Mirrors the encoding of [`InnerPuzzleMemo`] without decoding nested inner puzzles.
#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct ShallowInnerPuzzleMemo<T> {
    _nonce: usize,
    restrictions: Vec<T>,
    #[clvm(rest)]
    kind: ShallowMemoKind<T>,
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
enum ShallowMemoKind<T> {
    Member(MemberMemo<T>),
    MofN(ShallowMofNMemo<T>),
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct ShallowMofNMemo<T> {
    _required: usize,
    items: Vec<T>,
}

/// Walks the memo tree without recursively decoding it, so that the full decode afterward has
/// bounded recursion depth and output size.
fn check_inner_puzzle_memo_bounds(
    allocator: &Allocator,
    node: NodePtr,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), DriverError> {
    if depth > MAX_MIPS_MEMO_DEPTH {
        return Err(DriverError::MipsMemoTooDeep);
    }

    let memo = ShallowInnerPuzzleMemo::<NodePtr>::from_clvm(allocator, node)?;

    *nodes += 1 + memo.restrictions.len();

    if *nodes > MAX_MIPS_MEMO_NODES {
        return Err(DriverError::MipsMemoTooLarge);
    }

    if let ShallowMemoKind::MofN(m_of_n) = memo.kind {
        for item in m_of_n.items {
            check_inner_puzzle_memo_bounds(allocator, item, depth + 1, nodes)?;
        }
    }

    Ok(())
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

    /// Note that an empty restriction list means there is no restrictions layer at all, so a
    /// restrictions layer with no validators can't be represented (the same is true of
    /// [`mips_puzzle_hash`](crate::mips_puzzle_hash)).
    pub fn inner_puzzle_hash(&self, top_level: bool) -> TreeHash {
        self.wrap_puzzle_hash(self.kind.inner_puzzle_hash(), top_level)
    }

    /// Like [`InnerPuzzleMemo::inner_puzzle_hash`], but fails if any nested M of N can't be spent
    /// as described (see [`MofNMemo::try_inner_puzzle_hash`]).
    pub fn try_inner_puzzle_hash(&self, top_level: bool) -> Result<TreeHash, DriverError> {
        Ok(self.wrap_puzzle_hash(self.kind.try_inner_puzzle_hash()?, top_level))
    }

    fn wrap_puzzle_hash(&self, inner_puzzle_hash: TreeHash, top_level: bool) -> TreeHash {
        let (member_validators, delegated_puzzle_validators) =
            split_restrictions(&self.restrictions);

        wrap_mips_puzzle_hash(
            self.nonce,
            member_validators,
            delegated_puzzle_validators,
            inner_puzzle_hash,
            top_level,
        )
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

    /// Only combinations that can be spent are recognized. The delegated puzzle wrapper enforcer
    /// must validate the delegated puzzle, whereas the timelock and force 1 of 2 puzzles inspect
    /// conditions, so they must be member condition validators.
    pub fn parse(&self, allocator: &Allocator, ctx: &MipsMemoContext) -> Option<ParsedRestriction> {
        if !self.member_condition_validator {
            return self.parse_delegated_puzzle_validator(allocator);
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
            // A revealed timelock of 0 is encoded as nil, the same as a hidden timelock.
            for &seconds in [seconds.unwrap_or(0)].iter().chain(ctx.timelocks.iter()) {
                let restriction = Timelock::new(seconds);

                if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedRestriction::Timelock(restriction));
                }
            }
        }

        None
    }

    fn parse_delegated_puzzle_validator(&self, allocator: &Allocator) -> Option<ParsedRestriction> {
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
            memo.restriction().curry_tree_hash().into(),
            memo.to_clvm(allocator)?,
        ))
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
            // A revealed timelock of 0 is encoded as nil, the same as a hidden timelock.
            for &seconds in [seconds.unwrap_or(0)].iter().chain(ctx.timelocks.iter()) {
                let wrapper = Timelock::new(seconds);

                if wrapper.curry_tree_hash() == self.puzzle_hash.into() {
                    return Some(ParsedWrapper::Timelock(wrapper));
                }
            }
        }

        if let Ok(opcode) = Option::<u16>::from_clvm(allocator, self.memo) {
            // A revealed opcode of 0 is encoded as nil, the same as a hidden opcode.
            for &opcode in [opcode.unwrap_or(0)].iter().chain(ctx.opcodes.iter()) {
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

    pub fn try_inner_puzzle_hash(&self) -> Result<TreeHash, DriverError> {
        match self {
            Self::Member(member) => Ok(member.puzzle_hash.into()),
            Self::MofN(m_of_n) => m_of_n.try_inner_puzzle_hash(),
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

        // The cache keeps this linear if the memo shares subtrees.
        if tree_hash_cached(allocator, self.memo, &mut TreeCache::default())
            == self.puzzle_hash.into()
        {
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
        let items = self
            .items
            .iter()
            .map(|member| member.inner_puzzle_hash(false))
            .collect();

        MofN::new(self.required, items).inner_puzzle_hash()
    }

    /// Like [`MofNMemo::inner_puzzle_hash`], but fails if `required` isn't between 1 and the
    /// number of items, or if any items have the same puzzle hash (which would let a single
    /// member count more than once towards `required`).
    pub fn try_inner_puzzle_hash(&self) -> Result<TreeHash, DriverError> {
        let items = self
            .items
            .iter()
            .map(|member| member.try_inner_puzzle_hash(false))
            .collect::<Result<Vec<_>, _>>()?;

        let m_of_n = MofN::new(self.required, items);
        m_of_n.validate()?;

        let mut seen = HashSet::with_capacity(m_of_n.items.len());

        if !m_of_n.items.iter().all(|item| seen.insert(*item)) {
            return Err(DriverError::DuplicateMofNMembers);
        }

        Ok(m_of_n.inner_puzzle_hash())
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
