use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_derive::conditions;
use nfts::TradePrice;

mod agg_sig;
mod announcements;
mod list;
mod nfts;

pub use announcements::*;
pub use list::*;

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
            ...memos: Memos<T>,
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
        TransferNft as Default {
            opcode: i8 if -10,
            launcher_id: Option<Bytes32>,
            trade_prices: Vec<TradePrice>,
            singleton_inner_puzzle_hash: Option<Bytes32>,
        },
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
            program: P,
            solution: S,
        },
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
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
