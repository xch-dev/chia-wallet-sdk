use chia_protocol::Bytes32;
use clvm_traits::{apply_constants, FromClvm, ToClvm};
use clvmr::NodePtr;

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct Softfork<T> {
    #[clvm(constant = 90)]
    pub opcode: u8,
    pub cost: u64,
    #[clvm(rest)]
    pub rest: T,
}

impl<T> Softfork<T> {
    pub fn new(cost: u64, rest: T) -> Self {
        Self { cost, rest }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct Remark<T = NodePtr> {
    #[clvm(constant = 1)]
    pub opcode: u8,
    #[clvm(rest)]
    pub rest: T,
}

impl<T> Remark<T> {
    pub fn new(rest: T) -> Self {
        Self { rest }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RunTail<P, S> {
    #[clvm(constant = 51)]
    pub opcode: u8,
    #[clvm(constant = ())]
    pub puzzle_hash: (),
    #[clvm(constant = -113)]
    pub magic_amount: i8,
    pub program: P,
    pub solution: S,
}

impl<P, S> RunTail<P, S> {
    pub fn new(program: P, solution: S) -> Self {
        Self { program, solution }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct MeltSingleton {
    #[clvm(constant = 51)]
    pub opcode: u8,
    #[clvm(constant = ())]
    pub puzzle_hash: (),
    #[clvm(constant = -113)]
    pub magic_amount: i8,
}

impl MeltSingleton {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct NewNftOwner {
    #[clvm(constant = -10)]
    pub opcode: i8,
    pub did_id: Option<Bytes32>,
    pub trade_prices: Vec<NftTradePrice>,
    pub did_inner_puzzle_hash: Option<Bytes32>,
}

impl NewNftOwner {
    pub fn new(
        did_id: Option<Bytes32>,
        trade_prices: Vec<NftTradePrice>,
        did_inner_puzzle_hash: Option<Bytes32>,
    ) -> Self {
        Self {
            did_id,
            trade_prices,
            did_inner_puzzle_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NftTradePrice {
    pub trade_price: u16,
    pub puzzle_hash: Bytes32,
}

impl NftTradePrice {
    pub fn new(trade_price: u16, puzzle_hash: Bytes32) -> Self {
        Self {
            trade_price,
            puzzle_hash,
        }
    }
}
