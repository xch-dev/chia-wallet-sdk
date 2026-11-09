use std::{collections::HashMap, fmt::Debug};

use chia_protocol::Bytes32;
use chia_sdk_types::{
    MerkleProof, MerkleTree, Mod,
    puzzles::{
        ACTION_LAYER_PUZZLE_HASH, ActionLayerArgs, DEFAULT_FINALIZER_PUZZLE_HASH,
        DefaultFinalizer1stCurryArgs, DefaultFinalizer2ndCurryArgs, RESERVE_FINALIZER_PUZZLE_HASH,
        RawActionLayerSolution, ReserveFinalizer1stCurryArgs, ReserveFinalizer2ndCurryArgs,
    },
    run_puzzle,
};
use clvm_traits::{FromClvm, ToClvm, clvm_tuple, match_tuple};
use clvm_utils::{CurriedProgram, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The finalizer of an [`ActionLayer`], which runs after all of the actions in a spend.
///
/// Both finalizers output the conditions emitted by the actions and recreate the action layer
/// with the same merkle root and the new state, with an amount of 1 and a memo of `hint`.
///
/// `P` is the type of [`Finalizer::Reserve::reserve_amount_from_state_program`]. It's unused by
/// [`Finalizer::Default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finalizer<P> {
    /// The default finalizer ([`DefaultFinalizer1stCurryArgs`]). Its finalizer solution is unused.
    Default {
        /// The hint added as a memo to the recreated action layer coin.
        hint: Bytes32,
    },
    /// The reserve finalizer ([`ReserveFinalizer1stCurryArgs`]), for apps that control a reserve
    /// coin, such as the CAT that holds a reward distributor's rewards.
    ///
    /// Besides recreating the action layer, it sends a message to the reserve coin to spend it.
    /// The message commits to a delegated puzzle that recreates the reserve at
    /// `reserve_inner_puzzle_hash` with the amount computed from the new state, and outputs every
    /// action condition whose opcode is `-42`, with that opcode removed. Those conditions are
    /// not output by the action layer itself. The reserve coin's inner puzzle is a
    /// [`P2DelegatedBySingletonLayer`](crate::P2DelegatedBySingletonLayer) controlled by this
    /// singleton.
    ///
    /// The finalizer solution is a [`ReserveFinalizerSolution`](chia_sdk_types::puzzles::ReserveFinalizerSolution)
    /// containing the reserve coin's parent coin id.
    Reserve {
        /// The full puzzle hash of the reserve coin, such as a CAT puzzle hash.
        reserve_full_puzzle_hash: Bytes32,
        /// The inner puzzle hash of the reserve coin. The recreated reserve uses it as its
        /// puzzle hash and hint.
        reserve_inner_puzzle_hash: Bytes32,
        /// A program run with the state as its solution that returns the reserve amount.
        ///
        /// It's run on the old state to find the reserve coin being spent, and on the new state
        /// to get the amount of the recreated reserve. The default program is `(f state)`
        /// ([`RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM`](chia_sdk_types::puzzles::RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM)).
        reserve_amount_from_state_program: P,
        /// The hint added as a memo to the recreated action layer coin.
        hint: Bytes32,
    },
}

/// The action layer [`Layer`] is a singleton inner puzzle that stores a state and can only be
/// spent by running actions from a fixed set of action puzzles.
///
/// The puzzle ([`ActionLayerArgs`]) is curried with a [`Finalizer`], the
/// [`merkle_root`](Self::merkle_root) of the allowed action puzzle hashes, and the current
/// [`state`](Self::state). Anyone can spend it, but only by revealing action puzzles along with
/// [`MerkleProof`]s that they are in the tree, so the actions themselves enforce the rules of the
/// app. A spend ([`ActionLayerSolution`]) must run at least one action.
///
/// Actions run in order. Each one is run with `((ephemeral_state . state) . action_solution)`
/// and returns `((new_ephemeral_state . new_state) . conditions)`. The ephemeral state starts as
/// nil in every spend and is dropped at the end of it, so actions can use it to share data within
/// a single spend. Afterwards, the [`Finalizer`] outputs the conditions emitted by the actions
/// and recreates the action layer with the same merkle root and the final state.
///
/// The `CATalog` and `XCHandles` registries and the reward distributor are built on this layer.
/// Their actions are in this module, for example [`CatalogRegisterAction`](crate::CatalogRegisterAction),
/// [`XchandlesRegisterAction`](crate::XchandlesRegisterAction), and
/// [`RewardDistributorStakeAction`](crate::RewardDistributorStakeAction). Each implements
/// [`SingletonAction`](crate::SingletonAction) and [`ToTreeHash`](clvm_utils::ToTreeHash) to get
/// its puzzle hash, which is a leaf of the merkle tree.
///
/// `S` is the state type and `P` is the type of
/// [`Finalizer::Reserve::reserve_amount_from_state_program`].
///
/// [`Layer::parse_puzzle`] returns `Ok(None)` if the puzzle isn't an action layer, and
/// [`DriverError::NonStandardLayer`] if it is one but its finalizer isn't a known
/// [`Finalizer`] curried as expected.
///
/// # Example
///
/// Create an action layer with a single action that replaces the state with its solution, then
/// spend it:
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{
///     ActionLayer, ActionLayerSolution, Finalizer, Layer, Puzzle, Spend, SpendContext,
/// };
/// use chia_sdk_types::Condition;
/// use clvm_traits::clvm_list;
/// use clvmr::NodePtr;
///
/// let ctx = &mut SpendContext::new();
///
/// // The action `(c (c 4 3) ())` returns `((ephemeral_state . action_solution) . ())`.
/// let action = ctx.alloc(&clvm_list!(4, clvm_list!(4, 4, 3), ()))?;
/// let action_hash: Bytes32 = ctx.tree_hash(action).into();
///
/// let hint = Bytes32::new([1; 32]);
/// let finalizer = Finalizer::Default { hint };
/// let layer = ActionLayer::<u64>::from_action_puzzle_hashes(&[action_hash], 7, finalizer);
///
/// // Constructing and parsing the puzzle round-trips.
/// let puzzle = layer.construct_puzzle(ctx)?;
/// let parsed = ActionLayer::<u64>::parse_puzzle(ctx, Puzzle::parse(ctx, puzzle))?;
/// assert_eq!(parsed, Some(layer.clone()));
///
/// // Run the action once to change the state to 42.
/// let proofs = layer
///     .get_proofs(&[action_hash], &[action_hash])
///     .expect("the action is in the merkle tree");
/// let action_solution = ctx.alloc(&42)?;
/// let solution = layer.construct_solution(
///     ctx,
///     ActionLayerSolution {
///         proofs,
///         action_spends: vec![Spend::new(action, action_solution)],
///         finalizer_solution: NodePtr::NIL,
///     },
/// )?;
/// assert_eq!(ActionLayer::<u64>::get_new_state(ctx, 7, solution)?, 42);
///
/// // The finalizer recreates the action layer with the new state.
/// let new_layer = ActionLayer::<u64>::new(layer.merkle_root, 42, layer.finalizer.clone());
/// let new_puzzle = new_layer.construct_puzzle(ctx)?;
/// let new_puzzle_hash: Bytes32 = ctx.tree_hash(new_puzzle).into();
///
/// let output = ctx.run(puzzle, solution)?;
/// let conditions = ctx.extract::<Vec<Condition>>(output)?;
/// assert!(conditions.iter().any(|condition| matches!(
///     condition,
///     Condition::CreateCoin(create_coin)
///         if create_coin.puzzle_hash == new_puzzle_hash && create_coin.amount == 1
/// )));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionLayer<S, P = ()> {
    /// The root of the merkle tree whose leaves are the allowed action puzzle hashes.
    pub merkle_root: Bytes32,
    /// The current state, which is passed to and updated by the actions.
    pub state: S,
    /// The finalizer that runs after the actions.
    pub finalizer: Finalizer<P>,
}

/// The solution of an [`ActionLayer`], which lists the actions to run.
///
/// `F` is the type of the finalizer solution. When parsed with [`Layer::parse_solution`], it's
/// a [`NodePtr`].
#[derive(Debug, Clone)]
pub struct ActionLayerSolution<F> {
    /// The merkle proofs for [`action_spends`](Self::action_spends), in the same order. Use
    /// [`ActionLayer::get_proofs`] to compute them.
    ///
    /// The serialized solution only includes the proof for the first use of each action puzzle.
    pub proofs: Vec<MerkleProof>,
    /// The action puzzles and solutions, in the order they run. Each action puzzle is revealed
    /// in full, and each solution excludes the state, which the action layer passes in.
    pub action_spends: Vec<Spend>,
    /// The solution of the [`Finalizer`].
    pub finalizer_solution: F,
}

impl<S, P> ActionLayer<S, P> {
    /// Creates a new [`ActionLayer`] from its merkle root, state, and finalizer.
    pub fn new(merkle_root: Bytes32, state: S, finalizer: Finalizer<P>) -> Self {
        Self {
            merkle_root,
            state,
            finalizer,
        }
    }

    /// Creates a new [`ActionLayer`] whose merkle root is computed from the allowed action
    /// puzzle hashes, which are the leaves of the merkle tree.
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

    /// Computes the merkle proofs for `action_spends_puzzle_hashes`, in the same order.
    ///
    /// The merkle tree is rebuilt from `action_puzzle_hashes`, so they must be the same leaves, in
    /// the same order, that were used to compute [`merkle_root`](Self::merkle_root). The puzzle
    /// hashes in `action_spends_puzzle_hashes` may repeat.
    ///
    /// Returns `None` if any of `action_spends_puzzle_hashes` isn't in `action_puzzle_hashes`.
    pub fn get_proofs(
        &self,
        action_puzzle_hashes: &[Bytes32],
        action_spends_puzzle_hashes: &[Bytes32],
    ) -> Option<Vec<MerkleProof>> {
        let merkle_tree = MerkleTree::new(action_puzzle_hashes);

        let proofs = action_spends_puzzle_hashes
            .iter()
            .filter_map(|puzzle_hash| {
                let proof = merkle_tree.proof(*puzzle_hash)?;

                Some(proof)
            })
            .collect::<Vec<_>>();

        if proofs.len() != action_spends_puzzle_hashes.len() {
            return None;
        }

        Some(proofs)
    }

    /// Returns the merkle root and state of an action layer puzzle, or `None` if it isn't one.
    ///
    /// Unlike [`Layer::parse_puzzle`], this doesn't parse or check the finalizer.
    ///
    /// # Errors
    ///
    /// Returns an error if the curried arguments or the state can't be parsed.
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

    /// Computes the state after a spend by running its actions, starting from `initial_state`.
    ///
    /// `action_layer_solution` is the serialized [`ActionLayerSolution`] of the spend. Each action
    /// is run off-chain with an ephemeral state that starts as nil, exactly as the puzzle does.
    /// The merkle proofs aren't checked, and the finalizer isn't run.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be parsed or an action fails or returns a value
    /// that can't be parsed.
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
                clvm_tuple!(state_incl_ephemeral, raw_action.solution).to_clvm(allocator)?;

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
    type Solution = ActionLayerSolution<NodePtr>;

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
        let solution =
            RawActionLayerSolution::<NodePtr, NodePtr, NodePtr>::from_clvm(allocator, solution)?;

        let mut actions = Vec::<NodePtr>::with_capacity(solution.solutions.len());
        let mut proofs = Vec::<MerkleProof>::with_capacity(solution.solutions.len());
        let mut selector_proofs = HashMap::<u32, MerkleProof>::new();

        for (selector, proof) in solution.selectors_and_proofs {
            let proof = if let Some(existing_proof) = selector_proofs.get(&selector) {
                existing_proof.clone()
            } else {
                let proof = proof.ok_or(DriverError::InvalidMerkleProof)?;
                selector_proofs.insert(selector, proof.clone());
                proof
            };

            proofs.push(proof);

            let mut index = 0;
            let mut remaining_selector = selector;
            while remaining_selector > 2 {
                index += 1;
                remaining_selector /= 2;
            }
            actions.push(solution.puzzles[index]);
        }

        let action_spends = solution
            .solutions
            .iter()
            .zip(actions.into_iter().rev())
            .map(|(action_solution, action_puzzle)| Spend::new(action_puzzle, *action_solution))
            .collect();
        let proofs = proofs.into_iter().rev().collect();

        Ok(ActionLayerSolution {
            proofs,
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
        let mut selectors_and_proofs = Vec::new();
        let mut solutions = Vec::new();

        for (spend, proof) in solution.action_spends.into_iter().zip(solution.proofs) {
            let puzzle_hash = ctx.tree_hash(spend.puzzle).into();
            if let Some(selector) = puzzle_to_selector.get(&puzzle_hash) {
                selectors_and_proofs.push((*selector, Some(proof.clone())));
            } else {
                puzzles.push(spend.puzzle);
                selectors_and_proofs.push((next_selector, Some(proof.clone())));
                puzzle_to_selector.insert(puzzle_hash, next_selector);

                next_selector = next_selector * 2 + 1;
            }

            solutions.push(spend.solution);
        }

        let mut proven_selectors = Vec::<u32>::new();
        let mut selectors_and_proofs: Vec<(u32, Option<MerkleProof>)> =
            selectors_and_proofs.into_iter().rev().collect();
        #[allow(clippy::needless_range_loop)]
        for i in 0..selectors_and_proofs.len() {
            let selector = selectors_and_proofs[i].0;

            if proven_selectors.contains(&selector) {
                selectors_and_proofs[i].1 = None;
            } else {
                proven_selectors.push(selector);
            }
        }

        Ok(RawActionLayerSolution {
            puzzles,
            selectors_and_proofs,
            solutions,
            finalizer_solution: solution.finalizer_solution,
        }
        .to_clvm(ctx)?)
    }
}
