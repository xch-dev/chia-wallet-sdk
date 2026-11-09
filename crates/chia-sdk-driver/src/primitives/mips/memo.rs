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

use crate::{
    DriverError, MofN, VAULT_FAST_FORWARD_MESSAGE_MODE, VAULT_MESSAGE_MODE, wrap_mips_puzzle_hash,
};

/// The maximum nesting depth of M of N memos accepted by [`MipsMemo::from_clvm_bounded`].
pub const MAX_MIPS_MEMO_DEPTH: usize = 32;

/// The maximum number of inner puzzle and restriction memos accepted by
/// [`MipsMemo::from_clvm_bounded`]. Shared subtrees are counted once per occurrence.
pub const MAX_MIPS_MEMO_NODES: usize = 10_000;

/// A CHIP-0043 memo that describes a MIPS custody puzzle, so that its structure can be
/// reconstructed from its hash.
///
/// The memo is encoded as `("CHIP-0043" inner_puzzle)`. Each part of the tree can reveal or hide
/// details (such as public keys), and hidden details can be filled in with a [`MipsMemoContext`]
/// when parsing.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{InnerPuzzleMemo, MemberMemo, MemoKind, MipsMemo, mips_puzzle_hash};
/// use chia_sdk_test::BlsPair;
/// use chia_sdk_types::{Mod, puzzles::BlsMember};
/// use clvmr::Allocator;
///
/// let mut allocator = Allocator::new();
/// let pair = BlsPair::default();
///
/// // Describe a custody puzzle with a single BLS member, revealing the public key.
/// let member = MemberMemo::bls(&mut allocator, pair.pk, false, false, true)?;
/// let memo = MipsMemo::new(InnerPuzzleMemo::new(0, Vec::new(), MemoKind::Member(member)));
///
/// let custody_hash = mips_puzzle_hash(0, Vec::new(), BlsMember::new(pair.pk).curry_tree_hash(), true);
/// assert!(memo.verify(custody_hash));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct MipsMemo<T = NodePtr> {
    /// The namespace of the memo, which is always `"CHIP-0043"`.
    #[clvm(constant = "CHIP-0043".to_string())]
    pub namespace: String,
    /// The memo for the top level inner puzzle.
    pub inner_puzzle: InnerPuzzleMemo<T>,
}

impl MipsMemo<NodePtr> {
    /// Creates a new [`MipsMemo`] for the given top level inner puzzle.
    pub fn new(inner_puzzle: InnerPuzzleMemo) -> Self {
        Self { inner_puzzle }
    }

    /// Calculates the top level puzzle hash that the memo describes, without validating it.
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

/// A memo describing a single MIPS inner puzzle, mirroring [`InnerPuzzleSpend`](crate::InnerPuzzleSpend).
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InnerPuzzleMemo<T = NodePtr> {
    /// The nonce of the inner puzzle's index wrapper.
    pub nonce: usize,
    /// The restrictions on the inner puzzle.
    pub restrictions: Vec<RestrictionMemo<T>>,
    /// The member or M of N inside of the inner puzzle.
    #[clvm(rest)]
    pub kind: MemoKind<T>,
}

impl InnerPuzzleMemo<NodePtr> {
    /// Creates a new [`InnerPuzzleMemo`].
    pub fn new(nonce: usize, restrictions: Vec<RestrictionMemo>, kind: MemoKind) -> Self {
        Self {
            nonce,
            restrictions,
            kind,
        }
    }

    /// Calculates the MIPS puzzle hash that the memo describes, like [`mips_puzzle_hash`](crate::mips_puzzle_hash).
    ///
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

/// A memo describing a restriction on a MIPS inner puzzle.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionMemo<T = NodePtr> {
    /// Whether this is a member condition validator, rather than a delegated puzzle validator.
    pub member_condition_validator: bool,
    /// The tree hash of the restriction puzzle.
    pub puzzle_hash: Bytes32,
    /// Information about the restriction, which is used to identify it when parsing.
    pub memo: T,
}

impl RestrictionMemo<NodePtr> {
    /// Creates a new [`RestrictionMemo`].
    pub fn new(member_condition_validator: bool, puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self {
            member_condition_validator,
            puzzle_hash,
            memo,
        }
    }

    /// Creates a delegated puzzle validator memo for an [`EnforceDelegatedPuzzleWrappers`]
    /// restriction, which enforces the given stack of wrappers.
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

    /// Creates a member condition validator memo for a [`Timelock`] restriction.
    ///
    /// If `reveal` is set, the number of seconds is included in the memo. Otherwise, it
    /// must be provided in [`MipsMemoContext::timelocks`] when parsing.
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

    /// Identifies the restriction described by this memo, or returns [`None`] if it's not recognized.
    ///
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

/// A memo describing a delegated puzzle wrapper, as part of an
/// [`RestrictionMemo::enforce_delegated_puzzle_wrappers`] restriction.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct WrapperMemo<T = NodePtr> {
    /// The tree hash of the wrapper puzzle.
    pub puzzle_hash: Bytes32,
    /// Information about the wrapper, which is used to identify it when parsing.
    pub memo: T,
}

impl WrapperMemo<NodePtr> {
    /// Creates a new [`WrapperMemo`].
    pub fn new(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self { puzzle_hash, memo }
    }

    /// Creates a memo for the `force_assert_coin_announcement` wrapper.
    pub fn force_assert_coin_announcement() -> Self {
        Self {
            puzzle_hash: FORCE_ASSERT_COIN_ANNOUNCEMENT_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    /// Creates a memo for the `force_coin_message` wrapper.
    pub fn force_coin_message() -> Self {
        Self {
            puzzle_hash: FORCE_COIN_MESSAGE_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    /// Creates a memo for the `prevent_multiple_create_coins` wrapper.
    pub fn prevent_multiple_create_coins() -> Self {
        Self {
            puzzle_hash: PREVENT_MULTIPLE_CREATE_COINS_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    /// Creates a memo for the `force_singleton_recreation` wrapper.
    pub fn force_singleton_recreation() -> Self {
        Self {
            puzzle_hash: FORCE_SINGLETON_RECREATION_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    /// Creates a memo for a [`Force1of2RestrictedVariable`] wrapper.
    ///
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

    /// Creates a memo for a [`Timelock`] wrapper.
    ///
    /// If `reveal` is set, the number of seconds is included in the memo. Otherwise, it
    /// must be provided in [`MipsMemoContext::timelocks`] when parsing.
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

    /// Creates a memo for a [`PreventConditionOpcode`] wrapper.
    ///
    /// If `reveal` is set, the opcode is included in the memo. Otherwise, it must be provided in
    /// [`MipsMemoContext::opcodes`] when parsing.
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

    /// Identifies the wrapper described by this memo, or returns [`None`] if it's not recognized.
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

/// The memo of a [`Force1of2RestrictedVariable`] restriction or wrapper, which contains its curried values.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Force1of2RestrictedVariableMemo<T = NodePtr> {
    /// The hash of the fixed left side of the 1 of 2.
    pub left_side_subtree_hash: Bytes32,
    /// The nonce used by the index wrappers of the 1 of 2 and its right side.
    pub nonce: usize,
    /// The restrictions applied to the new right side.
    pub restrictions: Vec<RestrictionMemo<T>>,
}

impl Force1of2RestrictedVariableMemo<NodePtr> {
    /// Creates a new [`Force1of2RestrictedVariableMemo`].
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

    /// Returns the [`Force1of2RestrictedVariable`] puzzle described by this memo.
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

/// The kind of puzzle inside of a MIPS inner puzzle memo, mirroring [`MipsSpendKind`](crate::MipsSpendKind).
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub enum MemoKind<T = NodePtr> {
    /// A member puzzle.
    Member(MemberMemo<T>),
    /// An M of N of other inner puzzles.
    MofN(MofNMemo<T>),
}

impl MemoKind<NodePtr> {
    /// Calculates the puzzle hash inside of the MIPS wrappers, without validating it.
    pub fn inner_puzzle_hash(&self) -> TreeHash {
        match self {
            Self::Member(member) => member.puzzle_hash.into(),
            Self::MofN(m_of_n) => m_of_n.inner_puzzle_hash(),
        }
    }

    /// Like [`MemoKind::inner_puzzle_hash`], but fails if a nested M of N is invalid
    /// (see [`MofNMemo::try_inner_puzzle_hash`]).
    pub fn try_inner_puzzle_hash(&self) -> Result<TreeHash, DriverError> {
        match self {
            Self::Member(member) => Ok(member.puzzle_hash.into()),
            Self::MofN(m_of_n) => m_of_n.try_inner_puzzle_hash(),
        }
    }
}

/// A memo describing a MIPS member puzzle.
///
/// For the constructors below, `fast_forward` selects the variant of the member that signs with
/// (or for singletons, sends messages to) the puzzle hash instead of the coin id, which is
/// compatible with singleton fast forward, and `reveal` controls whether the key or hash is
/// included in the memo. If it's not revealed, it must be provided in the [`MipsMemoContext`]
/// when parsing.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MemberMemo<T = NodePtr> {
    /// The tree hash of the member puzzle.
    pub puzzle_hash: Bytes32,
    /// Information about the member, which is used to identify it when parsing.
    pub memo: T,
}

impl MemberMemo<NodePtr> {
    /// Creates a new [`MemberMemo`].
    pub fn new(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self { puzzle_hash, memo }
    }

    /// Creates a memo for a secp256k1 member ([`K1Member`] or [`K1MemberPuzzleAssert`]).
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

    /// Creates a memo for a secp256r1 member ([`R1Member`] or [`R1MemberPuzzleAssert`]).
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

    /// Creates a memo for a BLS member.
    ///
    /// If `taproot` is set, this is a [`BlsTaprootMember`] (or [`BlsTaprootMemberPuzzleAssert`]).
    /// Otherwise, it's a [`BlsMember`] (or [`BlsMemberPuzzleAssert`]).
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

    /// Creates a memo for a passkey member ([`PasskeyMember`] or [`PasskeyMemberPuzzleAssert`]).
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

    /// Creates a memo for a singleton member.
    ///
    /// If `fast_forward` is set, this is a [`SingletonMemberWithMode`] that uses puzzle hash to
    /// puzzle hash messages. Otherwise, it's a [`SingletonMember`].
    pub fn singleton(
        allocator: &mut Allocator,
        launcher_id: Bytes32,
        fast_forward: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if fast_forward {
                SingletonMemberWithMode::new(launcher_id, VAULT_FAST_FORWARD_MESSAGE_MODE)
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

    /// Creates a memo for a [`FixedPuzzleMember`], which requires the delegated puzzle to have the given hash.
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

    /// Identifies the member described by this memo, or returns [`None`] if it's not recognized.
    ///
    /// The known member types are tried with the revealed value and the values in the context. If
    /// none of them match, the memo is treated as the full puzzle reveal of a custom member.
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

/// A memo describing an M of N MIPS puzzle, mirroring [`MofN`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNMemo<T = NodePtr> {
    /// The number of items that must be spent.
    pub required: usize,
    /// The memos of the items, which are not top level.
    pub items: Vec<InnerPuzzleMemo<T>>,
}

impl MofNMemo<NodePtr> {
    /// Creates a new [`MofNMemo`].
    pub fn new(required: usize, items: Vec<InnerPuzzleMemo>) -> Self {
        Self { required, items }
    }

    /// Calculates the hash of the M of N puzzle, without validating it. See [`MofN::inner_puzzle_hash`].
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

/// Known values that are tried when parsing memos, to identify members, restrictions, and wrappers
/// whose details weren't revealed in the memo.
///
/// The default context has no keys, hashes, or timelocks, but includes the message and announcement
/// opcodes for [`PreventConditionOpcode`], and the singleton message modes that vaults use
/// ([`VAULT_FAST_FORWARD_MESSAGE_MODE`] and [`VAULT_MESSAGE_MODE`]).
#[derive(Debug, Clone)]
pub struct MipsMemoContext {
    /// Known secp256k1 public keys.
    pub k1: Vec<K1PublicKey>,
    /// Known secp256r1 public keys, used for both R1 and passkey members.
    pub r1: Vec<R1PublicKey>,
    /// Known BLS public keys.
    pub bls: Vec<PublicKey>,
    /// Known hashes, used for singleton launcher ids and fixed puzzle hashes.
    pub hashes: Vec<Bytes32>,
    /// Known timelock durations, in seconds.
    pub timelocks: Vec<u64>,
    /// Known condition opcodes for [`PreventConditionOpcode`] wrappers.
    pub opcodes: Vec<u16>,
    /// Known message modes for [`SingletonMemberWithMode`] members.
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
            singleton_modes: vec![VAULT_FAST_FORWARD_MESSAGE_MODE, VAULT_MESSAGE_MODE],
        }
    }
}
