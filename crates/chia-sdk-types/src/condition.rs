use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_derive::conditions;
use nfts::TradePrice;

mod agg_sig;
mod announcements;
mod consensus;
mod encoding;
mod list;
mod messages;
mod nfts;

pub use announcements::*;
pub use consensus::*;
pub use encoding::*;
pub use list::*;
pub use messages::*;

// Each field is one of the following:
// * `name: Type` is a single argument.
// * `name?: Type` is a single argument, which may be absent if the list ends before it.
// * `..name: Vec<Type>` is every argument until the list ends.
// * `...name: Type` is the rest of the list.
// * Any of these followed by `if value` is a constant, which isn't stored.
//
// Unless the last field is a `...` field, a trailing `rest` field (of type `R`) is added to keep
// any arguments after the known ones, so that every condition serializes back to the same value.
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
            memos?: Memos<T>,
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
            ..data: Vec<T>,
        },
        ReceiveMessage<T> {
            opcode: i8 if 67,
            mode: u8,
            message: Bytes,
            ..data: Vec<T>,
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
        TransferNft as Default {
            opcode: i8 if -10,
            launcher_id: Option<Bytes32>,
            trade_prices: Vec<TradePrice>,
            singleton_inner_puzzle_hash: Option<Bytes32>,
        },
        /// A `CREATE_COIN` with the magic -113 amount, followed by a TAIL reveal and solution.
        ///
        /// The meaning of this condition depends on the outer puzzle that consumes it. The CAT layer runs
        /// the revealed TAIL, but the singleton layer treats any -113 output as a melt, regardless of what
        /// follows the amount. Use [`Condition::is_magic_create_coin`] to detect either interpretation.
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: Option<Bytes32>,
            magic_amount: i8 if -113,
            program: P,
            solution: S,
        },
        /// A `CREATE_COIN` with the magic -113 amount, and nothing following it.
        ///
        /// The meaning of this condition depends on the outer puzzle that consumes it. The singleton layer
        /// melts the singleton, but the CAT layer treats any -113 output as a TAIL reveal (which fails here,
        /// since there's no TAIL). A singleton will also melt if it outputs a [`Condition::RunCatTail`].
        /// Use [`Condition::is_magic_create_coin`] to detect either interpretation.
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: Option<Bytes32>,
            magic_amount: i8 if -113,
            // Nothing can follow the magic amount, so that this can't be confused with `RunCatTail`.
            ...rest: () if (),
        },
        UpdateNftMetadata<P, S> as Copy {
            opcode: i8 if -24,
            updater_puzzle_reveal: P,
            updater_solution: S,
        },
        UpdateDatastoreMerkleRoot {
            opcode: i8 if -13,
            new_merkle_root: Bytes32,
            ...memos: Vec<Bytes>,
        },
    }
}

impl<T> Condition<T> {
    /// Whether this is a `CREATE_COIN` with the magic -113 amount, which outer puzzles intercept rather
    /// than passing to consensus. This is either a [`Condition::RunCatTail`] or a [`Condition::MeltSingleton`],
    /// but the outer puzzle determines what it means, so code that handles one should usually handle both.
    pub fn is_magic_create_coin(&self) -> bool {
        self.is_run_cat_tail() || self.is_melt_singleton()
    }
}

#[cfg(test)]
mod tests;
