use chia_bls::PublicKey;
use chia_protocol::Bytes;
use clvm_traits::{FromClvm, ToClvm};

use super::Condition;

/// The kind of an `AGG_SIG_*` condition, which decides what is appended to the message
/// before it's signed.
///
/// The discriminant of each variant is the condition's opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm, Hash)]
#[repr(u8)]
#[clvm(atom)]
pub enum AggSigKind {
    /// `AGG_SIG_PARENT`, which appends the parent coin id. See
    /// [`AggSigParent`](super::conditions::AggSigParent).
    Parent = 43,
    /// `AGG_SIG_PUZZLE`, which appends the puzzle hash. See
    /// [`AggSigPuzzle`](super::conditions::AggSigPuzzle).
    Puzzle = 44,
    /// `AGG_SIG_AMOUNT`, which appends the amount. See
    /// [`AggSigAmount`](super::conditions::AggSigAmount).
    Amount = 45,
    /// `AGG_SIG_PUZZLE_AMOUNT`, which appends the puzzle hash and amount. See
    /// [`AggSigPuzzleAmount`](super::conditions::AggSigPuzzleAmount).
    PuzzleAmount = 46,
    /// `AGG_SIG_PARENT_AMOUNT`, which appends the parent coin id and amount. See
    /// [`AggSigParentAmount`](super::conditions::AggSigParentAmount).
    ParentAmount = 47,
    /// `AGG_SIG_PARENT_PUZZLE`, which appends the parent coin id and puzzle hash. See
    /// [`AggSigParentPuzzle`](super::conditions::AggSigParentPuzzle).
    ParentPuzzle = 48,
    /// `AGG_SIG_UNSAFE`, which appends nothing. See
    /// [`AggSigUnsafe`](super::conditions::AggSigUnsafe).
    Unsafe = 49,
    /// `AGG_SIG_ME`, which appends the coin id. See [`AggSigMe`](super::conditions::AggSigMe).
    Me = 50,
}

/// Any of the `AGG_SIG_*` conditions, with the kind stored as a field instead of in the type.
///
/// This is useful when you need to handle every signature condition the same way, such as when
/// collecting the signatures a spend requires. Use [`Condition::into_agg_sig`] to convert from a
/// [`Condition`].
///
/// ## Example
///
/// ```rust
/// # use chia_bls::PublicKey;
/// # use chia_protocol::Bytes;
/// # use chia_sdk_types::{Condition, conditions::{AggSig, AggSigKind}};
/// let message = Bytes::from(vec![1, 2, 3]);
/// let condition: Condition = Condition::agg_sig_me(PublicKey::default(), message.clone());
/// assert!(condition.is_agg_sig());
///
/// let agg_sig = condition.into_agg_sig().expect("this is an agg sig condition");
/// assert_eq!(agg_sig.kind, AggSigKind::Me);
/// assert_eq!(agg_sig.message, message);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AggSig {
    /// The kind of signature condition, which is also its opcode.
    pub kind: AggSigKind,
    /// The public key that must sign the message.
    pub public_key: PublicKey,
    /// The message to sign, before any coin or network data required by the kind is appended.
    pub message: Bytes,
}

impl AggSig {
    /// Creates a new [`AggSig`] from its kind, public key, and message.
    pub fn new(kind: AggSigKind, public_key: PublicKey, message: Bytes) -> Self {
        Self {
            kind,
            public_key,
            message,
        }
    }
}

impl<T> Condition<T> {
    /// Converts any `AGG_SIG_*` condition into an [`AggSig`], or returns [`None`] if this isn't a
    /// signature condition.
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

    /// Returns `true` if this is any of the `AGG_SIG_*` conditions.
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
