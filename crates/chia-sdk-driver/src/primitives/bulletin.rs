use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, Conditions, conditions::Remark, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{BulletinLayer, DriverError, HashedPtr, Layer, Puzzle, Spend, SpendContext};

/// A message posted by a [`Bulletin`], encoded as the list `(topic content)` in a `REMARK` condition.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BulletinMessage {
    /// The topic of the message, such as a slash-separated path.
    pub topic: String,
    /// The content of the message.
    pub content: String,
}

impl BulletinMessage {
    /// Creates a new [`BulletinMessage`].
    pub fn new(topic: String, content: String) -> Self {
        Self { topic, content }
    }
}

/// A zero-amount coin that publishes [`BulletinMessage`]s on-chain as `REMARK` conditions when spent.
///
/// The coin's puzzle is a [`BulletinLayer`] wrapping an inner puzzle, which marks the coin as a
/// bulletin and controls who can post the messages.
///
/// # Example
///
/// ```
/// use chia_sdk_driver::{Bulletin, BulletinMessage, Puzzle, SpendContext, SpendWithConditions, StandardLayer};
/// use chia_sdk_test::Simulator;
///
/// let mut sim = Simulator::new();
/// let ctx = &mut SpendContext::new();
///
/// let alice = sim.bls(0);
/// let alice_p2 = StandardLayer::new(alice.pk);
///
/// let message = BulletinMessage::new("greetings".to_string(), "Hello, world!".to_string());
/// let (parent_conditions, bulletin) =
///     Bulletin::create(alice.coin.coin_id(), alice.puzzle_hash, vec![message])?;
/// alice_p2.spend(ctx, alice.coin, parent_conditions)?;
///
/// // Spend the bulletin coin with Alice's puzzle to post the messages.
/// let conditions = bulletin.conditions(ctx)?;
/// let inner_spend = alice_p2.spend_with_conditions(ctx, conditions)?;
/// bulletin.spend(ctx, inner_spend)?;
/// sim.spend_coins(ctx.take(), &[alice.sk])?;
///
/// // Read the messages back from the bulletin coin's spend.
/// let coin_spend = sim.coin_spend(bulletin.coin.coin_id()).unwrap();
/// let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
/// let puzzle = Puzzle::parse(ctx, puzzle);
/// let solution = ctx.alloc(&coin_spend.solution)?;
/// let (parsed, ..) = Bulletin::parse(ctx, coin_spend.coin, puzzle, solution)?.unwrap();
/// assert_eq!(parsed, bulletin);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bulletin {
    /// The bulletin coin.
    pub coin: Coin,
    /// The hash of the inner puzzle wrapped by the [`BulletinLayer`], which must be revealed to spend the coin.
    pub hidden_puzzle_hash: Bytes32,
    /// The messages that the bulletin posts (or posted) when spent.
    pub messages: Vec<BulletinMessage>,
}

impl Bulletin {
    /// Creates a new [`Bulletin`] from its coin, inner puzzle hash, and messages.
    pub fn new(coin: Coin, hidden_puzzle_hash: Bytes32, messages: Vec<BulletinMessage>) -> Self {
        Self {
            coin,
            hidden_puzzle_hash,
            messages,
        }
    }

    /// Prepares a new bulletin coin with the given inner puzzle hash and messages.
    ///
    /// Returns the conditions that the parent coin must output to create the zero-amount bulletin
    /// coin, and the [`Bulletin`]. The bulletin still needs to be spent with [`Bulletin::spend`] to
    /// publish the messages.
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

    /// Returns a `REMARK` condition for each message, which the inner spend should output.
    pub fn conditions(&self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        let mut conditions = Conditions::new();

        for message in &self.messages {
            conditions.push(Remark::new(ctx.alloc(message)?));
        }

        Ok(conditions)
    }

    /// Spends the bulletin coin with the given inner spend, adding it to the [`SpendContext`].
    ///
    /// The inner spend's puzzle must hash to [`hidden_puzzle_hash`](Self::hidden_puzzle_hash), and it
    /// should output the [`Bulletin::conditions`]. This isn't checked.
    pub fn spend(&self, ctx: &mut SpendContext, spend: Spend) -> Result<(), DriverError> {
        let layer = BulletinLayer::new(spend.puzzle);
        let coin_spend = layer.construct_coin_spend(ctx, self.coin, spend.solution)?;
        ctx.insert(coin_spend);
        Ok(())
    }

    /// Parses a [`Bulletin`] and its inner spend from a coin spend.
    ///
    /// The messages are read from the `REMARK` conditions output by the inner spend. Remarks that
    /// aren't valid [`BulletinMessage`]s are skipped. Returns [`None`] if the puzzle isn't a bulletin.
    pub fn parse(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<(Self, Puzzle, NodePtr)>, DriverError> {
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

        let bulletin = Self::new(
            coin,
            bulletin_layer.inner_puzzle.tree_hash().into(),
            messages,
        );

        Ok(Some((
            bulletin,
            Puzzle::parse(allocator, bulletin_layer.inner_puzzle.ptr()),
            bulletin_solution,
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

        let (parsed_bulletin, ..) =
            Bulletin::parse(&mut ctx, coin_spend.coin, puzzle, solution)?.unwrap();

        assert_eq!(parsed_bulletin, bulletin);

        Ok(())
    }
}
