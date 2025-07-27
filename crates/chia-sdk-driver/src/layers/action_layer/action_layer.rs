use std::{collections::HashMap, fmt::Debug};

use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{
        ActionLayerArgs, DefaultFinalizer1stCurryArgs, DefaultFinalizer2ndCurryArgs,
        RawActionLayerSolution, ReserveFinalizer1stCurryArgs, ReserveFinalizer2ndCurryArgs,
        ACTION_LAYER_PUZZLE_HASH, DEFAULT_FINALIZER_PUZZLE_HASH, RESERVE_FINALIZER_PUZZLE_HASH,
    },
    run_puzzle, MerkleTree, Mod,
};
use clvm_traits::{clvm_list, match_tuple, FromClvm, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finalizer<P> {
    Default {
        hint: Bytes32,
    },
    Reserve {
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: P,
        hint: Bytes32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionLayer<S, P = ()> {
    pub merkle_root: Bytes32,
    pub state: S,
    pub finalizer: Finalizer<P>,
}

#[derive(Debug, Clone)]
pub struct ActionLayerSolution<R, F> {
    pub partial_tree_reveal: R,
    pub action_spends: Vec<Spend>,
    pub finalizer_solution: F,
}

impl<S, P> ActionLayer<S, P> {
    pub fn new(merkle_root: Bytes32, state: S, finalizer: Finalizer<P>) -> Self {
        Self {
            merkle_root,
            state,
            finalizer,
        }
    }

    pub fn from_action_puzzle_hashes(
        leaves: &[Bytes32],
        state: S,
        finalizer: Finalizer<P>,
    ) -> Self {
        let merkle_root = MerkleTree::new(leaves).root();

        Self {
            merkle_root,
            state,
            finalizer,
        }
    }

    pub fn extract_merkle_root_and_state(
        allocator: &Allocator,
        inner_puzzle: Puzzle,
    ) -> Result<Option<(Bytes32, S)>, DriverError>
    where
        S: FromClvm<Allocator>,
    {
        let Some(puzzle) = inner_puzzle.as_curried() else {
            return Ok(None);
        };

        if inner_puzzle.mod_hash() != ACTION_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = ActionLayerArgs::<NodePtr, S>::from_clvm(allocator, puzzle.args)?;

        Ok(Some((args.merkle_root, args.state)))
    }

    pub fn get_new_state(
        allocator: &mut Allocator,
        initial_state: S,
        action_layer_solution: NodePtr,
    ) -> Result<S, DriverError>
    where
        S: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let solution = ActionLayer::<S, NodePtr>::parse_solution(allocator, action_layer_solution)?;

        let mut state_incl_ephemeral: (NodePtr, S) = (NodePtr::NIL, initial_state);
        for raw_action in solution.action_spends {
            let actual_solution =
                clvm_list!(state_incl_ephemeral, raw_action.solution).to_clvm(allocator)?;

            let output = run_puzzle(allocator, raw_action.puzzle, actual_solution)?;

            (state_incl_ephemeral, _) =
                <match_tuple!((NodePtr, S), NodePtr)>::from_clvm(allocator, output)?;
        }

        Ok(state_incl_ephemeral.1)
    }
}

impl<S, P> Layer for ActionLayer<S, P>
where
    S: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    P: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    type Solution = ActionLayerSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != ACTION_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = ActionLayerArgs::<NodePtr, S>::from_clvm(allocator, puzzle.args)?;
        let finalizer_2nd_curry =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, args.finalizer);
        let Ok(finalizer_2nd_curry) = finalizer_2nd_curry else {
            return Ok(None);
        };

        let finalizer_1st_curry = Puzzle::from_clvm(allocator, finalizer_2nd_curry.program)?;
        let Some(finalizer_1st_curry) = finalizer_1st_curry.as_curried() else {
            return Ok(None);
        };

        match finalizer_1st_curry.mod_hash {
            DEFAULT_FINALIZER_PUZZLE_HASH => {
                let finalizer_2nd_curry_args =
                    DefaultFinalizer2ndCurryArgs::from_clvm(allocator, finalizer_2nd_curry.args)?;
                let finalizer_1st_curry_args =
                    DefaultFinalizer1stCurryArgs::from_clvm(allocator, finalizer_1st_curry.args)?;

                let expected_self_hash = DefaultFinalizer1stCurryArgs {
                    action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
                    hint: finalizer_1st_curry_args.hint,
                }
                .curry_tree_hash()
                .into();
                if finalizer_1st_curry.mod_hash != DEFAULT_FINALIZER_PUZZLE_HASH
                    || finalizer_1st_curry_args.action_layer_mod_hash
                        != ACTION_LAYER_PUZZLE_HASH.into()
                    || finalizer_2nd_curry_args.finalizer_self_hash != expected_self_hash
                {
                    return Err(DriverError::NonStandardLayer);
                }

                Ok(Some(Self {
                    merkle_root: args.merkle_root,
                    state: args.state,
                    finalizer: Finalizer::Default {
                        hint: finalizer_1st_curry_args.hint,
                    },
                }))
            }
            RESERVE_FINALIZER_PUZZLE_HASH => {
                let finalizer_2nd_curry_args =
                    ReserveFinalizer2ndCurryArgs::from_clvm(allocator, finalizer_2nd_curry.args)?;
                let finalizer_1st_curry_args = ReserveFinalizer1stCurryArgs::<NodePtr>::from_clvm(
                    allocator,
                    finalizer_1st_curry.args,
                )?;

                let reserve_amount_from_state_program_hash = tree_hash(
                    allocator,
                    finalizer_1st_curry_args.reserve_amount_from_state_program,
                );

                if finalizer_1st_curry.mod_hash != RESERVE_FINALIZER_PUZZLE_HASH
                    || finalizer_1st_curry_args.action_layer_mod_hash
                        != ACTION_LAYER_PUZZLE_HASH.into()
                    || finalizer_2nd_curry_args.finalizer_self_hash
                        != ReserveFinalizer1stCurryArgs::<TreeHash>::curry_tree_hash(
                            finalizer_1st_curry_args.reserve_full_puzzle_hash,
                            finalizer_1st_curry_args.reserve_inner_puzzle_hash,
                            reserve_amount_from_state_program_hash,
                            finalizer_1st_curry_args.hint,
                        )
                        .into()
                {
                    return Err(DriverError::NonStandardLayer);
                }

                let reserve_amount_from_state_program = <P>::from_clvm(
                    allocator,
                    finalizer_1st_curry_args.reserve_amount_from_state_program,
                )?;

                Ok(Some(Self {
                    merkle_root: args.merkle_root,
                    state: args.state,
                    finalizer: Finalizer::Reserve {
                        reserve_full_puzzle_hash: finalizer_1st_curry_args.reserve_full_puzzle_hash,
                        reserve_inner_puzzle_hash: finalizer_1st_curry_args
                            .reserve_inner_puzzle_hash,
                        reserve_amount_from_state_program,
                        hint: finalizer_1st_curry_args.hint,
                    },
                }))
            }
            _ => Err(DriverError::NonStandardLayer),
        }
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let solution = RawActionLayerSolution::<NodePtr, NodePtr, NodePtr, NodePtr>::from_clvm(
            allocator, solution,
        )?;

        let mut action_spends = Vec::<Spend>::with_capacity(solution.selectors_and_solutions.len());

        for (selector, action_solution) in solution.selectors_and_solutions {
            let mut index = 0;
            let mut remaining_selector = selector;
            while remaining_selector > 2 {
                index += 1;
                remaining_selector /= 2;
            }
            action_spends.push(Spend::new(solution.puzzles[index], action_solution));
        }

        Ok(ActionLayerSolution {
            partial_tree_reveal: solution.partial_tree_reveal,
            action_spends,
            finalizer_solution: solution.finalizer_solution,
        })
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let finalizer_1st_curry = match &self.finalizer {
            Finalizer::Default { hint } => ctx.curry(DefaultFinalizer1stCurryArgs::new(*hint))?,
            Finalizer::Reserve {
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program,
                hint,
            } => ctx.curry(ReserveFinalizer1stCurryArgs::<P>::new(
                *reserve_full_puzzle_hash,
                *reserve_inner_puzzle_hash,
                reserve_amount_from_state_program.clone(),
                *hint,
            ))?,
        };

        let finalizer = match &self.finalizer {
            Finalizer::Default { hint } => CurriedProgram {
                program: finalizer_1st_curry,
                args: DefaultFinalizer2ndCurryArgs::new(*hint),
            }
            .to_clvm(ctx)?,
            Finalizer::Reserve {
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program,
                hint,
            } => {
                let reserve_amount_from_state_program =
                    ctx.alloc(&reserve_amount_from_state_program)?;
                let reserve_amount_from_state_program_hash =
                    ctx.tree_hash(reserve_amount_from_state_program);

                CurriedProgram {
                    program: finalizer_1st_curry,
                    args: ReserveFinalizer2ndCurryArgs::new(
                        *reserve_full_puzzle_hash,
                        *reserve_inner_puzzle_hash,
                        &reserve_amount_from_state_program_hash,
                        *hint,
                    ),
                }
                .to_clvm(ctx)?
            }
        };

        ctx.curry(ActionLayerArgs::<NodePtr, S>::new(
            finalizer,
            self.merkle_root,
            self.state.clone(),
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let mut puzzle_to_selector = HashMap::<Bytes32, u32>::new();
        let mut next_selector = 2;

        let mut puzzles = Vec::new();
        let mut selectors_and_solutions = Vec::with_capacity(solution.action_spends.len());

        for spend in solution.action_spends {
            let puzzle_hash = ctx.tree_hash(spend.puzzle).into();
            if let Some(selector) = puzzle_to_selector.get(&puzzle_hash) {
                selectors_and_solutions.push((*selector, spend.solution));
            } else {
                puzzles.push(spend.puzzle);
                selectors_and_solutions.push((next_selector, spend.solution));
                puzzle_to_selector.insert(puzzle_hash, next_selector);

                next_selector = next_selector * 2 + 1;
            }
        }
        Ok(RawActionLayerSolution {
            puzzles,
            partial_tree_reveal: solution.partial_tree_reveal,
            selectors_and_solutions,
            finalizer_solution: solution.finalizer_solution,
        }
        .to_clvm(ctx)?)
    }
}
