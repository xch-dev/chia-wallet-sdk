use chia_protocol::Bytes32;
use chia_sdk_driver::Cat;

use crate::{Puzzle, Spend};

pub trait CatExt {}

impl CatExt for Cat {}

#[derive(Clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
}

impl From<CatSpend> for chia_sdk_driver::CatSpend {
    fn from(value: CatSpend) -> Self {
        chia_sdk_driver::CatSpend::new(value.cat, value.spend.into())
    }
}

#[derive(Clone)]
pub struct ParsedCat {
    pub asset_id: Bytes32,
    pub p2_puzzle: Puzzle,
}
