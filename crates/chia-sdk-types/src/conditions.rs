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

use clvm_traits::{FromClvm, ToClvm};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub enum Condition<T> {
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
    RunTail(RunTail<T, T>),
    MeltSingleton(MeltSingleton),
    NewNftOwner(NewNftOwner),
    Softfork(Softfork<T>),
}
