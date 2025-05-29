mod parsed_member;
mod parsed_restriction;
mod parsed_wrapper;

use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
pub use parsed_member::*;
pub use parsed_restriction::*;
pub use parsed_wrapper::*;

use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{
        BlsMember, BlsTaprootMember, EnforceDelegatedPuzzleWrappers, FixedPuzzleMember,
        Force1of2RestrictedVariable, K1Member, K1MemberPuzzleAssert, PasskeyMember,
        PasskeyMemberPuzzleAssert, PreventConditionOpcode, R1Member, R1MemberPuzzleAssert,
        SingletonMember, Timelock, FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH,
        FORCE_COIN_MESSAGE_PUZZLE_HASH, PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
    },
    Mod,
};
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_traits::{apply_constants, FromClvm, ToClvm};
use clvm_utils::TreeHash;
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

    pub fn force_1_of_2_restricted_variable(
        allocator: &mut Allocator,
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            false,
            Force1of2RestrictedVariable::new(
                left_side_subtree_hash,
                nonce,
                member_validator_list_hash,
                delegated_puzzle_validator_list_hash,
            )
            .curry_tree_hash()
            .into(),
            Force1of2RestrictedVariableMemo::new(
                left_side_subtree_hash,
                nonce,
                member_validator_list_hash,
                delegated_puzzle_validator_list_hash,
            )
            .to_clvm(allocator)?,
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

        let memos = wrapper_memos
            .iter()
            .map(|item| item.memo)
            .collect::<Vec<NodePtr>>();

        Ok(Self::new(
            false,
            EnforceDelegatedPuzzleWrappers::new(&wrapper_stack)
                .curry_tree_hash()
                .into(),
            memos.to_clvm(allocator)?,
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
        if let Ok(items) = Vec::<WrapperMemo>::from_clvm(allocator, self.memo) {
            let wrapper_stack: Vec<TreeHash> = items
                .iter()
                .map(|item| TreeHash::from(item.puzzle_hash))
                .collect();

            let restriction = EnforceDelegatedPuzzleWrappers::new(&wrapper_stack);

            if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(
                    restriction,
                    items.iter().map(|item| item.memo).collect(),
                ));
            }
        }

        if let Ok(memo) = Force1of2RestrictedVariableMemo::from_clvm(allocator, self.memo) {
            let restriction = Force1of2RestrictedVariable::new(
                memo.left_side_subtree_hash,
                memo.nonce,
                memo.member_validator_list_hash,
                memo.delegated_puzzle_validator_list_hash,
            );

            if restriction.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedRestriction::Force1of2RestrictedVariable(restriction));
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
            puzzle_hash: FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn force_coin_message() -> Self {
        Self {
            puzzle_hash: FORCE_COIN_MESSAGE_PUZZLE_HASH.into(),
            memo: NodePtr::NIL,
        }
    }

    pub fn prevent_multiple_create_coins() -> Self {
        Self {
            puzzle_hash: PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH.into(),
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
        if self.puzzle_hash == FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH.into() {
            return Some(ParsedWrapper::ForceAssertCoinAnnouncement);
        }

        if self.puzzle_hash == FORCE_COIN_MESSAGE_PUZZLE_HASH.into() {
            return Some(ParsedWrapper::ForceCoinMessage);
        }

        if self.puzzle_hash == PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH.into() {
            return Some(ParsedWrapper::PreventMultipleCreateCoins);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Force1of2RestrictedVariableMemo {
    pub left_side_subtree_hash: Bytes32,
    pub nonce: usize,
    pub member_validator_list_hash: Bytes32,
    pub delegated_puzzle_validator_list_hash: Bytes32,
}

impl Force1of2RestrictedVariableMemo {
    pub fn new(
        left_side_subtree_hash: Bytes32,
        nonce: usize,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
    ) -> Self {
        Self {
            left_side_subtree_hash,
            nonce,
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub enum MemoKind<T = NodePtr> {
    Member(MemberMemo<T>),
    MofN(MofNMemo<T>),
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
        taproot: bool,
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            if taproot {
                BlsTaprootMember::new(public_key).curry_tree_hash().into()
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
        reveal: bool,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            SingletonMember::new(launcher_id).curry_tree_hash().into(),
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

            let member = BlsTaprootMember::new(public_key);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::BlsTaproot(member));
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

            let member = FixedPuzzleMember::new(hash);
            if member.curry_tree_hash() == self.puzzle_hash.into() {
                return Some(ParsedMember::FixedPuzzle(member));
            }
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
}

#[derive(Debug, Clone)]
pub struct MipsMemoContext {
    pub k1: Vec<K1PublicKey>,
    pub r1: Vec<R1PublicKey>,
    pub bls: Vec<PublicKey>,
    pub hashes: Vec<Bytes32>,
    pub timelocks: Vec<u64>,
    pub opcodes: Vec<u16>,
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
        }
    }
}
