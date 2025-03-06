use std::ops::Index;

use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_derive::conditions;
use clvm_traits::{FromClvm, ToClvm, ToClvmError};

mod agg_sig;

pub use agg_sig::*;
use clvmr::{Allocator, NodePtr};

conditions! {
    pub enum Condition<T> {
        Remark<T> as Copy {
            opcode: i8 if 1,
            ...rest: T,
        },
        AggSigParent {
            opcode: i8 if 43,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigPuzzle {
            opcode: i8 if 44,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigAmount {
            opcode: i8 if 45,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigPuzzleAmount {
            opcode: i8 if 46,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigParentAmount {
            opcode: i8 if 47,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigParentPuzzle {
            opcode: i8 if 48,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigUnsafe {
            opcode: i8 if 49,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigMe {
            opcode: i8 if 50,
            public_key: PublicKey,
            message: Bytes,
        },
        CreateCoin<T> as Copy {
            opcode: i8 if 51,
            puzzle_hash: Bytes32,
            amount: u64,
            ...memos: Option<Memos<T>>,
        },
        ReserveFee as Copy {
            opcode: i8 if 52,
            amount: u64,
        },
        CreateCoinAnnouncement {
            opcode: i8 if 60,
            message: Bytes,
        },
        AssertCoinAnnouncement as Copy {
            opcode: i8 if 61,
            announcement_id: Bytes32,
        },
        CreatePuzzleAnnouncement {
            opcode: i8 if 62,
            message: Bytes,
        },
        AssertPuzzleAnnouncement as Copy {
            opcode: i8 if 63,
            announcement_id: Bytes32,
        },
        AssertConcurrentSpend as Copy {
            opcode: i8 if 64,
            coin_id: Bytes32,
        },
        AssertConcurrentPuzzle as Copy {
            opcode: i8 if 65,
            puzzle_hash: Bytes32,
        },
        SendMessage<T> {
            opcode: i8 if 66,
            mode: u8,
            message: Bytes,
            ...data: Vec<T>,
        },
        ReceiveMessage<T> {
            opcode: i8 if 67,
            mode: u8,
            message: Bytes,
            ...data: Vec<T>,
        },
        AssertMyCoinId as Copy {
            opcode: i8 if 70,
            coin_id: Bytes32,
        },
        AssertMyParentId as Copy {
            opcode: i8 if 71,
            parent_id: Bytes32,
        },
        AssertMyPuzzleHash as Copy {
            opcode: i8 if 72,
            puzzle_hash: Bytes32,
        },
        AssertMyAmount as Copy {
            opcode: i8 if 73,
            amount: u64,
        },
        AssertMyBirthSeconds as Copy {
            opcode: i8 if 74,
            seconds: u64,
        },
        AssertMyBirthHeight as Copy {
            opcode: i8 if 75,
            height: u32,
        },
        AssertEphemeral as Default + Copy {
            opcode: i8 if 76,
        },
        AssertSecondsRelative as Copy {
            opcode: i8 if 80,
            seconds: u64,
        },
        AssertSecondsAbsolute as Copy {
            opcode: i8 if 81,
            seconds: u64,
        },
        AssertHeightRelative as Copy {
            opcode: i8 if 82,
            height: u32,
        },
        AssertHeightAbsolute as Copy {
            opcode: i8 if 83,
            height: u32,
        },
        AssertBeforeSecondsRelative as Copy {
            opcode: i8 if 84,
            seconds: u64,
        },
        AssertBeforeSecondsAbsolute as Copy {
            opcode: i8 if 85,
            seconds: u64,
        },
        AssertBeforeHeightRelative as Copy {
            opcode: i8 if 86,
            height: u32,
        },
        AssertBeforeHeightAbsolute as Copy {
            opcode: i8 if 87,
            height: u32,
        },
        Softfork<T> as Copy {
            opcode: i8 if 90,
            cost: u64,
            ...rest: T,
        },
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
        },
        TransferNft as Default {
            opcode: i8 if -10,
            did_id: Option<Bytes32>,
            trade_prices: Vec<TradePrice>,
            did_inner_puzzle_hash: Option<Bytes32>,
        },
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
            program: P,
            solution: S,
        },
        UpdateNftMetadata<P, S> as Copy {
            opcode: i8 if -24,
            updater_puzzle_reveal: P,
            updater_solution: S,
        },
        UpdateDataStoreMerkleRoot {
            opcode: i8 if -13,
            new_merkle_root: Bytes32,
            ...memos: Vec<Bytes>,
        },
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Memos<T> {
    pub value: T,
}

impl<T> Memos<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }

    pub fn some(value: T) -> Option<Self> {
        Some(Self { value })
    }
}

impl Memos<NodePtr> {
    pub fn hint(allocator: &mut Allocator, hint: Bytes32) -> Result<Self, ToClvmError> {
        Ok(Self {
            value: [hint].to_clvm(allocator)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataInfo<M> {
    pub new_metadata: M,
    pub new_updater_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataOutput<M, C> {
    pub metadata_info: NewMetadataInfo<M>,
    pub conditions: C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TradePrice {
    pub amount: u64,
    pub puzzle_hash: Bytes32,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub struct Conditions<T = NodePtr> {
    conditions: Vec<Condition<T>>,
}

impl<T> Default for Conditions<T> {
    fn default() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }
}

impl Conditions<NodePtr> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Conditions<T> {
    pub fn len(&self) -> usize {
        self.conditions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }

    pub fn with(mut self, condition: impl Into<Condition<T>>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    pub fn extend(mut self, conditions: impl IntoIterator<Item = impl Into<Condition<T>>>) -> Self {
        self.conditions
            .extend(conditions.into_iter().map(Into::into));
        self
    }

    pub fn extend_from_slice(mut self, conditions: &[Condition<T>]) -> Self
    where
        T: Clone,
    {
        self.conditions.extend_from_slice(conditions);
        self
    }
}

impl<T> Index<usize> for Conditions<T> {
    type Output = Condition<T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.conditions[index]
    }
}

impl<T> AsRef<[Condition<T>]> for Conditions<T> {
    fn as_ref(&self) -> &[Condition<T>] {
        &self.conditions
    }
}

impl<T> IntoIterator for Conditions<T> {
    type Item = Condition<T>;
    type IntoIter = std::vec::IntoIter<Condition<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.conditions.into_iter()
    }
}
