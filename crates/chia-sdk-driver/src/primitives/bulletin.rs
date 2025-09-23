use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::{run_puzzle, Condition, Conditions};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{
    BulletinLayer, DriverError, HashedPtr, Layer, Puzzle, SpendContext, SpendWithConditions,
};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BulletinMessage {
    pub topic: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bulletin {
    pub coin: Coin,
    pub hidden_puzzle_hash: Bytes32,
    pub messages: Vec<BulletinMessage>,
}

impl Bulletin {
    pub fn new(coin: Coin, hidden_puzzle_hash: Bytes32, messages: Vec<BulletinMessage>) -> Self {
        Self {
            coin,
            hidden_puzzle_hash,
            messages,
        }
    }

    pub fn create<I>(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        inner: &I,
        messages: &[BulletinMessage],
    ) -> Result<Conditions, DriverError>
    where
        I: SpendWithConditions,
    {
        todo!()
    }

    pub fn parse(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let Some(bulletin_layer) = BulletinLayer::<HashedPtr>::parse_puzzle(allocator, puzzle)?
        else {
            return Ok(None);
        };

        let bulletin_solution = BulletinLayer::<NodePtr>::parse_solution(allocator, solution)?;

        let output = run_puzzle(
            allocator,
            bulletin_layer.inner_puzzle.ptr(),
            bulletin_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut messages = Vec::new();

        for condition in conditions {
            let Some(remark) = condition.into_remark() else {
                continue;
            };

            if let Ok(message) = BulletinMessage::from_clvm(allocator, remark.rest) {
                messages.push(message);
            }
        }

        Ok(Some(Self::new(
            coin,
            bulletin_layer.inner_puzzle.tree_hash().into(),
            messages,
        )))
    }
}
