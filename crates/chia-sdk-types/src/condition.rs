use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use paste::paste;

mod agg_sig;
mod announcements;
mod coin_info;
mod concurrent;
mod output;
mod puzzles;
mod time;

pub use agg_sig::*;
pub use announcements::*;
pub use coin_info::*;
pub use concurrent::*;
pub use output::*;
pub use puzzles::*;
pub use time::*;

use crate::Conditions;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub enum Condition<T = NodePtr> {
    Remark(Remark<T>),
    AggSig(AggSig),
    CreateCoin(CreateCoin),
    ReserveFee(ReserveFee),
    CreateCoinAnnouncement(CreateCoinAnnouncement),
    AssertCoinAnnouncement(AssertCoinAnnouncement),
    CreatePuzzleAnnouncement(CreatePuzzleAnnouncement),
    AssertPuzzleAnnouncement(AssertPuzzleAnnouncement),
    AssertConcurrentSpend(AssertConcurrentSpend),
    AssertConcurrentPuzzle(AssertConcurrentPuzzle),
    AssertMyCoinId(AssertMyCoinId),
    AssertMyParentId(AssertMyParentId),
    AssertMyPuzzleHash(AssertMyPuzzleHash),
    AssertMyAmount(AssertMyAmount),
    AssertMyBirthSeconds(AssertMyBirthSeconds),
    AssertMyBirthHeight(AssertMyBirthHeight),
    AssertEphemeral(AssertEphemeral),
    AssertSecondsRelative(AssertSecondsRelative),
    AssertSecondsAbsolute(AssertSecondsAbsolute),
    AssertHeightRelative(AssertHeightRelative),
    AssertHeightAbsolute(AssertHeightAbsolute),
    AssertBeforeSecondsRelative(AssertBeforeSecondsRelative),
    AssertBeforeSecondsAbsolute(AssertBeforeSecondsAbsolute),
    AssertBeforeHeightRelative(AssertBeforeHeightRelative),
    AssertBeforeHeightAbsolute(AssertBeforeHeightAbsolute),
    Softfork(Softfork<T>),
    Other(T),
}

macro_rules! into {
    ( $name:ident -> $condition:ident $( < $generic:ident > )? { $( $field:ident : $ty:ty ),* $(,)? } ) => {
        paste! {
            impl<T> Condition<T> {
                pub fn $name( $( $field: $ty, )* ) -> Self {
                    Condition::$condition($condition {
                        $( $field, )*
                    })
                }

                pub fn [<into_ $name>](self) -> Option<$condition $( <$generic> )? > {
                    match self {
                        Condition::$condition(condition) => Some(condition),
                        _ => None,
                    }
                }

                pub fn [<is_ $name>](&self) -> bool {
                    matches!(self, Condition::$condition(_))
                }

                pub fn [<as_ $name>](&self) -> Option<&$condition $( <$generic> )? > {
                    match self {
                        Condition::$condition(condition) => Some(condition),
                        _ => None,
                    }
                }
            }

            impl<T> From<$condition $( <$generic> )? > for Condition<T> {
                fn from(condition: $condition $( <$generic> )? ) -> Self {
                    Condition::$condition(condition)
                }
            }

            impl<T> TryFrom<Condition<T>> for $condition $( <$generic> )? {
                type Error = Condition<T>;

                fn try_from(condition: Condition<T>) -> Result<Self, Self::Error> {
                    match condition {
                        Condition::$condition(condition) => Ok(condition),
                        _ => Err(condition),
                    }
                }
            }

            impl<T> Conditions<T> {
                pub fn $name( self, $( $field: $ty, )* ) -> Self {
                    self.with(Condition::$condition($condition {
                        $( $field, )*
                    }))
                }
            }
        }
    };
}

into!(remark -> Remark<T> { rest: T });
into!(agg_sig -> AggSig { kind: AggSigKind, public_key: PublicKey, message: Bytes });
into!(create_coin -> CreateCoin { puzzle_hash: Bytes32, amount: u64, memos: Vec<Bytes> });
into!(reserve_fee -> ReserveFee { amount: u64 });
into!(create_coin_announcement -> CreateCoinAnnouncement { message: Bytes });
into!(assert_coin_announcement -> AssertCoinAnnouncement { announcement_id: Bytes32 });
into!(create_puzzle_announcement -> CreatePuzzleAnnouncement { message: Bytes });
into!(assert_puzzle_announcement -> AssertPuzzleAnnouncement { announcement_id: Bytes32 });
into!(assert_concurrent_spend -> AssertConcurrentSpend { coin_id: Bytes32 });
into!(assert_concurrent_puzzle -> AssertConcurrentPuzzle { puzzle_hash: Bytes32 });
into!(assert_my_coin_id -> AssertMyCoinId { coin_id: Bytes32 });
into!(assert_my_parent_id -> AssertMyParentId { parent_id: Bytes32 });
into!(assert_my_puzzle_hash -> AssertMyPuzzleHash { puzzle_hash: Bytes32 });
into!(assert_my_amount -> AssertMyAmount { amount: u64 });
into!(assert_my_birth_seconds -> AssertMyBirthSeconds { seconds: u64 });
into!(assert_my_birth_height -> AssertMyBirthHeight { height: u32 });
into!(assert_ephemeral -> AssertEphemeral {});
into!(assert_seconds_relative -> AssertSecondsRelative { seconds: u64 });
into!(assert_seconds_absolute -> AssertSecondsAbsolute { seconds: u64 });
into!(assert_height_relative -> AssertHeightRelative { height: u32 });
into!(assert_height_absolute -> AssertHeightAbsolute { height: u32 });
into!(assert_before_seconds_relative -> AssertBeforeSecondsRelative { seconds: u64 });
into!(assert_before_seconds_absolute -> AssertBeforeSecondsAbsolute { seconds: u64 });
into!(assert_before_height_relative -> AssertBeforeHeightRelative { height: u32 });
into!(assert_before_height_absolute -> AssertBeforeHeightAbsolute { height: u32 });
into!(softfork -> Softfork<T> { cost: u64, rest: T });

impl<T> Condition<T> {
    pub fn other(ptr: T) -> Self {
        Condition::Other(ptr)
    }

    pub fn into_other(self) -> Option<T> {
        match self {
            Condition::Other(ptr) => Some(ptr),
            _ => None,
        }
    }

    pub fn is_other(&self) -> bool {
        matches!(self, Condition::Other(_))
    }

    pub fn as_other(&self) -> Option<&T> {
        match self {
            Condition::Other(ptr) => Some(ptr),
            _ => None,
        }
    }
}
