use chia_protocol::Bytes32;
use clvm_traits::{apply_constants, FromClvm, ToClvm};

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

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct NewNftOwner {
    #[clvm(constant = -10)]
    pub opcode: i8,
    pub new_owner: Option<Bytes32>,
    pub trade_prices_list: Vec<NftTradePrice>,
    pub new_did_p2_puzzle_hash: Option<Bytes32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NftTradePrice {
    pub trade_price: u16,
    pub puzzle_hash: Bytes32,
}
