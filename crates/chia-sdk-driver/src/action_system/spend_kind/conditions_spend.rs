use std::{collections::HashSet, mem};

use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::{Condition, Conditions, Mod, conditions::CreateCoin, puzzles::RevocationArgs};
use clvmr::NodePtr;

use crate::{DriverError, Output, OutputSet, SpendContext};

#[derive(Debug, Default, Clone)]
pub struct ConditionsSpend {
    conditions: Conditions,
    outputs: HashSet<Output>,
}

impl ConditionsSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_conditions(&mut self, conditions: Conditions) {
        for condition in conditions {
            if let Some(create_coin) = condition.as_create_coin() {
                let output = Output::new(create_coin.puzzle_hash, create_coin.amount);
                self.outputs.insert(output);
            }
            self.conditions.push(condition);
        }
    }

    /// Wraps the puzzle hash of every created coin in the revocation layer with the given hidden
    /// puzzle hash, and makes sure the unwrapped puzzle hash is the first memo, so that the child
    /// can be recognized as revocable by [`Cat::child_from_p2_create_coin`](crate::Cat::child_from_p2_create_coin).
    ///
    /// This is needed when spending a revocable CAT with its hidden puzzle, since the revocation
    /// layer passes the created coins through as-is in that case. Outputs are still tracked by
    /// their unwrapped puzzle hash, which is equivalent since the wrapping is injective.
    pub fn wrap_for_revocation(
        &mut self,
        ctx: &mut SpendContext,
        hidden_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        for condition in mem::take(&mut self.conditions) {
            let Condition::CreateCoin(create_coin) = condition else {
                self.conditions.push(condition);
                continue;
            };

            let memos = hint_first(ctx, create_coin.puzzle_hash, create_coin.memos)?;

            self.conditions.push(Condition::CreateCoin(CreateCoin::new(
                RevocationArgs::new(hidden_puzzle_hash, create_coin.puzzle_hash)
                    .curry_tree_hash()
                    .into(),
                create_coin.amount,
                memos,
            )));
        }

        Ok(())
    }

    pub fn finish(self) -> Conditions {
        self.conditions
    }
}

fn hint_first(
    ctx: &mut SpendContext,
    hint: Bytes32,
    memos: Memos<NodePtr>,
) -> Result<Memos<NodePtr>, DriverError> {
    let Memos::Some(list) = memos else {
        return ctx.hint(hint);
    };

    if ctx
        .extract::<(Bytes32, NodePtr)>(list)
        .is_ok_and(|(first, _)| first == hint)
    {
        return Ok(memos);
    }

    Ok(Memos::Some(ctx.alloc(&(hint, list))?))
}

impl OutputSet for ConditionsSpend {
    fn has_output(&self, output: &Output) -> bool {
        self.outputs.contains(output)
    }

    fn can_run_cat_tail(&self) -> bool {
        !self.conditions.iter().any(Condition::is_run_cat_tail)
    }

    fn missing_singleton_output(&self) -> bool {
        !self.conditions.iter().any(|condition| {
            condition.is_melt_singleton()
                || condition
                    .as_create_coin()
                    .is_some_and(|create_coin| create_coin.amount % 2 == 1)
        })
    }
}
