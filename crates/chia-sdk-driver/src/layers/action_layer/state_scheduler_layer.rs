use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    Condition, Conditions,
    puzzles::{STATE_SCHEDULER_PUZZLE_HASH, StateSchedulerLayerArgs, StateSchedulerLayerSolution},
};
use clvm_traits::{FromClvm, clvm_quote, match_quote};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext, XchandlesRegistryReceivedMessagePrefix};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSchedulerLayer {
    pub receiver_singleton_struct_hash: Bytes32,
    pub new_state_hash: Bytes32,
    pub required_timestamp: u64,
    pub new_puzzle_hash: Bytes32,
}

impl StateSchedulerLayer {
    pub fn new(
        receiver_singleton_struct_hash: Bytes32,
        new_state_hash: Bytes32,
        required_timestamp: u64,
        new_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            receiver_singleton_struct_hash,
            new_state_hash,
            required_timestamp,
            new_puzzle_hash,
        }
    }
}

impl Layer for StateSchedulerLayer {
    type Solution = StateSchedulerLayerSolution<()>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != STATE_SCHEDULER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = StateSchedulerLayerArgs::<Bytes, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into() {
            return Err(DriverError::NonStandardLayer);
        }

        let (_q, conditions) =
            <match_quote!(Vec<Condition<NodePtr>>)>::from_clvm(allocator, args.inner_puzzle)?;
        let (
            Some(Condition::AssertSecondsAbsolute(assert_seconds_condition)),
            Some(Condition::CreateCoin(create_coin_condition)),
        ) = conditions
            .into_iter()
            .fold(
                (None, None),
                |(assert_seconds, create_coin), cond| match cond {
                    Condition::AssertSecondsAbsolute(_) if assert_seconds.is_none() => {
                        (Some(cond), create_coin)
                    }
                    Condition::CreateCoin(_) if create_coin.is_none() => {
                        (assert_seconds, Some(cond))
                    }
                    _ => (assert_seconds, create_coin),
                },
            )
        else {
            return Err(DriverError::NonStandardLayer);
        };

        let prefix_and_message = args.prefix_and_message;
        if prefix_and_message.len() != 33 {
            return Err(DriverError::NonStandardLayer);
        }
        let new_state_hash = Bytes32::new(
            prefix_and_message[1..]
                .try_into()
                .map_err(|_| DriverError::NonStandardLayer)?,
        );

        Ok(Some(Self {
            receiver_singleton_struct_hash: args.receiver_singleton_struct_hash,
            new_state_hash,
            required_timestamp: assert_seconds_condition.seconds,
            new_puzzle_hash: create_coin_condition.puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        StateSchedulerLayerSolution::from_clvm(allocator, solution).map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let base_conditions = Conditions::new()
            .create_coin(self.new_puzzle_hash, 1, Memos::None)
            .assert_seconds_absolute(self.required_timestamp);

        let inner_puzzle = ctx.alloc(&clvm_quote!(base_conditions))?;

        ctx.curry(StateSchedulerLayerArgs::<Bytes, NodePtr> {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            receiver_singleton_struct_hash: self.receiver_singleton_struct_hash,
            prefix_and_message: XchandlesRegistryReceivedMessagePrefix::update_state(
                self.new_state_hash.into(),
            )
            .into(),
            inner_puzzle,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Bytes, Bytes32};
    use chia_puzzle_types::Memos;
    use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
    use chia_sdk_types::{
        Condition, Conditions,
        puzzles::{STATE_SCHEDULER_PUZZLE_HASH, StateSchedulerLayerArgs},
    };
    use clvm_traits::{clvm_quote, match_quote};
    use clvm_utils::ToTreeHash;

    use crate::{Layer, Puzzle, SpendContext, XchandlesRegistryReceivedMessagePrefix};

    use super::*;

    fn sample_layer() -> StateSchedulerLayer {
        StateSchedulerLayer::new(
            Bytes32::new([1; 32]),
            Bytes32::new([2; 32]),
            1_700_000_000,
            Bytes32::new([3; 32]),
        )
    }

    #[test]
    fn test_state_scheduler_layer_roundtrip() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let layer = sample_layer();

        let ptr = layer.construct_puzzle(&mut ctx)?;
        let puzzle = Puzzle::parse(&ctx, ptr);
        let roundtrip = StateSchedulerLayer::parse_puzzle(&ctx, puzzle)?.expect("parse");

        assert_eq!(roundtrip, layer);
        assert_eq!(
            hex::encode(ctx.tree_hash(ptr)),
            hex::encode(layer_tree_hash(&layer))
        );

        Ok(())
    }

    #[test]
    fn test_state_scheduler_layer_emits_assert_seconds_absolute() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let layer = sample_layer();

        let ptr = layer.construct_puzzle(&mut ctx)?;
        let puzzle = Puzzle::parse(&ctx, ptr).as_curried().expect("curried");
        let args = StateSchedulerLayerArgs::<Bytes, NodePtr>::from_clvm(&ctx, puzzle.args)?;
        let (_q, conditions) =
            <match_quote!(Vec<Condition<NodePtr>>)>::from_clvm(&ctx, args.inner_puzzle)?;

        assert!(conditions.iter().any(|c| {
            matches!(
                c,
                Condition::AssertSecondsAbsolute(cond) if cond.seconds == layer.required_timestamp
            )
        }));
        assert!(
            conditions
                .iter()
                .all(|c| !matches!(c, Condition::AssertHeightAbsolute(_)))
        );

        Ok(())
    }

    #[test]
    fn test_state_scheduler_layer_rejects_height_absolute() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let layer = sample_layer();

        let height_conditions = Conditions::new()
            .create_coin(layer.new_puzzle_hash, 1, Memos::None)
            .assert_height_absolute(42);
        let inner_puzzle = ctx.alloc(&clvm_quote!(height_conditions))?;
        let ptr = ctx.curry(StateSchedulerLayerArgs::<chia_protocol::Bytes, NodePtr> {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            receiver_singleton_struct_hash: layer.receiver_singleton_struct_hash,
            prefix_and_message: XchandlesRegistryReceivedMessagePrefix::update_state(
                layer.new_state_hash.into(),
            )
            .into(),
            inner_puzzle,
        })?;

        let puzzle = Puzzle::parse(&ctx, ptr);
        let err = StateSchedulerLayer::parse_puzzle(&ctx, puzzle).unwrap_err();
        assert!(matches!(err, DriverError::NonStandardLayer));

        Ok(())
    }

    #[test]
    fn test_outer_module_hash_unchanged() {
        assert_eq!(
            hex::encode(STATE_SCHEDULER_PUZZLE_HASH),
            "8811d56e9efd2c9f449ea10cb00e00417b372f46d9d3a00ddf632f292de7e2c3"
        );
    }

    fn layer_tree_hash(layer: &StateSchedulerLayer) -> clvm_utils::TreeHash {
        let prefix_and_message: chia_protocol::Bytes =
            XchandlesRegistryReceivedMessagePrefix::update_state(layer.new_state_hash.into())
                .into();
        StateSchedulerLayerArgs::<clvm_utils::TreeHash, _>::curry_tree_hash(
            layer.receiver_singleton_struct_hash,
            prefix_and_message.tree_hash(),
            &clvm_quote!(vec![
                Condition::<()>::create_coin(layer.new_puzzle_hash, 1, Memos::None),
                Condition::assert_seconds_absolute(layer.required_timestamp),
            ]),
        )
    }
}
