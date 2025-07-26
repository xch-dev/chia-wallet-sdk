use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    puzzles::{
        P2MOfNDelegateDirectArgs, P2MOfNDelegateDirectSolution,
        P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH,
    },
    Condition, Conditions,
};
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MOfNLayer {
    pub m: usize,
    pub public_key_list: Vec<PublicKey>,
}

impl Layer for MOfNLayer {
    type Solution = P2MOfNDelegateDirectSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2MOfNDelegateDirectArgs::new(
            self.m,
            self.public_key_list.clone(),
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2MOfNDelegateDirectArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            m: args.m,
            public_key_list: args.public_key_list,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2MOfNDelegateDirectSolution::from_clvm(
            allocator, solution,
        )?)
    }
}

impl MOfNLayer {
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

    pub fn ensure_non_replayable(
        conditions: Conditions,
        coin_id: Bytes32,
        genesis_challenge: NodePtr,
    ) -> Conditions {
        let found_condition = conditions.clone().into_iter().find(|c| {
            matches!(c, Condition::AssertMyCoinId(..))
                || matches!(c, Condition::AssertMyParentId(..))
        });

        if found_condition.is_some() {
            conditions
        } else {
            conditions.assert_my_coin_id(coin_id)
        }
        .remark(genesis_challenge)
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        conditions: Conditions,
        used_pubkeys: &[PublicKey],
        genesis_challenge: Bytes32,
    ) -> Result<(), DriverError> {
        let genesis_challenge = ctx.alloc(&genesis_challenge)?;
        let spend = self.spend_with_conditions(
            ctx,
            Self::ensure_non_replayable(conditions, coin.coin_id(), genesis_challenge),
            used_pubkeys,
        )?;
        ctx.spend(coin, spend)
    }

    pub fn spend_with_conditions(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
        used_pubkeys: &[PublicKey],
    ) -> Result<Spend, DriverError> {
        let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        self.construct_spend(
            ctx,
            P2MOfNDelegateDirectSolution {
                selectors: P2MOfNDelegateDirectArgs::selectors_for_used_pubkeys(
                    &self.public_key_list,
                    used_pubkeys,
                ),
                delegated_puzzle,
                delegated_solution: NodePtr::NIL,
            },
        )
    }
}

impl ToTreeHash for MOfNLayer {
    fn tree_hash(&self) -> TreeHash {
        P2MOfNDelegateDirectArgs::curry_tree_hash(self.m, self.public_key_list.clone())
    }
}
