use chia_bls::PublicKey;
use chia_protocol::Bytes;
use clvm_traits::{FromClvm, ToClvm};

use super::Condition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm, Hash)]
#[repr(u8)]
#[clvm(atom)]
pub enum AggSigKind {
    Parent = 43,
    Puzzle = 44,
    Amount = 45,
    PuzzleAmount = 46,
    ParentAmount = 47,
    ParentPuzzle = 48,
    Unsafe = 49,
    Me = 50,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AggSig {
    pub kind: AggSigKind,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSig {
    pub fn new(kind: AggSigKind, public_key: PublicKey, message: Bytes) -> Self {
        Self {
            kind,
            public_key,
            message,
        }
    }
}

impl<T> Condition<T> {
    pub fn into_agg_sig(self) -> Option<AggSig> {
        match self {
            Condition::AggSigParent(inner) => Some(AggSig::new(
                AggSigKind::Parent,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigPuzzle(inner) => Some(AggSig::new(
                AggSigKind::Puzzle,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigAmount(inner) => Some(AggSig::new(
                AggSigKind::Amount,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigPuzzleAmount(inner) => Some(AggSig::new(
                AggSigKind::PuzzleAmount,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigParentAmount(inner) => Some(AggSig::new(
                AggSigKind::ParentAmount,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigParentPuzzle(inner) => Some(AggSig::new(
                AggSigKind::ParentPuzzle,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigUnsafe(inner) => Some(AggSig::new(
                AggSigKind::Unsafe,
                inner.public_key,
                inner.message,
            )),
            Condition::AggSigMe(inner) => {
                Some(AggSig::new(AggSigKind::Me, inner.public_key, inner.message))
            }
            _ => None,
        }
    }

    pub fn is_agg_sig(&self) -> bool {
        matches!(
            self,
            Condition::AggSigParent(..)
                | Condition::AggSigPuzzle(..)
                | Condition::AggSigAmount(..)
                | Condition::AggSigPuzzleAmount(..)
                | Condition::AggSigParentAmount(..)
                | Condition::AggSigParentPuzzle(..)
                | Condition::AggSigUnsafe(..)
                | Condition::AggSigMe(..)
        )
    }
}
