use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, Conditions, conditions::Remark, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{BulletinLayer, DriverError, HashedPtr, Layer, Puzzle, Spend, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BulletinMessage {
    pub topic: String,
    pub content: String,
}

impl BulletinMessage {
    pub fn new(topic: String, content: String) -> Self {
        Self { topic, content }
    }
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

    pub fn create(
        parent_coin_id: Bytes32,
        hidden_puzzle_hash: Bytes32,
        messages: Vec<BulletinMessage>,
    ) -> Result<(Conditions, Bulletin), DriverError> {
        let puzzle_hash = BulletinLayer::new(TreeHash::from(hidden_puzzle_hash))
            .tree_hash()
            .into();

        let parent_conditions = Conditions::new().create_coin(puzzle_hash, 0, Memos::None);

        let bulletin = Bulletin::new(
            Coin::new(parent_coin_id, puzzle_hash, 0),
            hidden_puzzle_hash,
            messages,
        );

        Ok((parent_conditions, bulletin))
    }

    pub fn conditions(&self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        let mut conditions = Conditions::new();

        for message in &self.messages {
            conditions.push(Remark::new(ctx.alloc(message)?));
        }

        Ok(conditions)
    }

    pub fn spend(&self, ctx: &mut SpendContext, spend: Spend) -> Result<(), DriverError> {
        let layer = BulletinLayer::new(spend.puzzle);
        let coin_spend = layer.construct_coin_spend(ctx, self.coin, spend.solution)?;
        ctx.insert(coin_spend);
        Ok(())
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;

    use crate::{SpendWithConditions, StandardLayer};

    use super::*;

    #[test]
    fn test_bulletin() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(0);
        let p2 = StandardLayer::new(alice.pk);

        let (parent_conditions, bulletin) = Bulletin::create(
            alice.coin.coin_id(),
            alice.puzzle_hash,
            vec![BulletinMessage::new(
                "animals/rabbit/vienna-blue".to_string(),
                "The Vienna Blue rabbit breed originally comes from Austria.".to_string(),
            )],
        )?;

        p2.spend(&mut ctx, alice.coin, parent_conditions)?;

        let conditions = bulletin.conditions(&mut ctx)?;
        let bulletin_spend = p2.spend_with_conditions(&mut ctx, conditions)?;
        bulletin.spend(&mut ctx, bulletin_spend)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let coin_spend = sim.coin_spend(bulletin.coin.coin_id()).unwrap();

        let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(&ctx, puzzle);
        let solution = ctx.alloc(&coin_spend.solution)?;

        let parsed_bulletin =
            Bulletin::parse(&mut ctx, coin_spend.coin, puzzle, solution)?.unwrap();

        assert_eq!(parsed_bulletin, bulletin);

        Ok(())
    }
}
