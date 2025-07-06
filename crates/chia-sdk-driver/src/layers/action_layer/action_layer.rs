use std::{collections::HashMap, fmt::Debug};

use chia::{
    clvm_traits::{FromClvm, ToClvm},
    clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, Spend, SpendContext},
    types::{run_puzzle, MerkleProof, MerkleTree},
};
use clvm_traits::{clvm_list, match_tuple};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::SpendContextExt;

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
pub struct ActionLayerSolution<F> {
    pub proofs: Vec<MerkleProof>,
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

    pub fn get_proofs(
        &self,
        action_puzzle_hashes: &[Bytes32],
        action_spends_puzzle_hashes: &[Bytes32],
    ) -> Option<Vec<MerkleProof>> {
        let merkle_tree = MerkleTree::new(action_puzzle_hashes);

        let proofs: Vec<MerkleProof> = action_spends_puzzle_hashes
            .iter()
            .filter_map(|puzzle_hash| {
                let proof = merkle_tree.proof(*puzzle_hash)?;

                Some(proof)
            })
            .collect();

        if proofs.len() != action_spends_puzzle_hashes.len() {
            return None;
        }

        Some(proofs)
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

                if finalizer_1st_curry.mod_hash != DEFAULT_FINALIZER_PUZZLE_HASH
                    || finalizer_1st_curry_args.action_layer_mod_hash
                        != ACTION_LAYER_PUZZLE_HASH.into()
                    || finalizer_2nd_curry_args.finalizer_self_hash
                        != DefaultFinalizer1stCurryArgs::curry_tree_hash(
                            finalizer_1st_curry_args.hint,
                        )
                        .into()
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

        for (selector, proof) in solution.selectors_and_proofs.into_iter() {
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
            actions.push(solution.puzzles[index as usize]);
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
            Finalizer::Default { hint } => CurriedProgram {
                program: ctx.default_finalizer_puzzle()?,
                args: DefaultFinalizer1stCurryArgs::new(*hint),
            }
            .to_clvm(ctx)?,
            Finalizer::Reserve {
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program,
                hint,
            } => CurriedProgram {
                program: ctx.reserve_finalizer_puzzle()?,
                args: ReserveFinalizer1stCurryArgs::<P>::new(
                    *reserve_full_puzzle_hash,
                    *reserve_inner_puzzle_hash,
                    reserve_amount_from_state_program.clone(),
                    *hint,
                ),
            }
            .to_clvm(ctx)?,
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
                        reserve_amount_from_state_program_hash,
                        *hint,
                    ),
                }
                .to_clvm(ctx)?
            }
        };

        Ok(CurriedProgram {
            program: ctx.action_layer_puzzle()?,
            args: ActionLayerArgs::<NodePtr, S>::new(
                finalizer,
                self.merkle_root,
                self.state.clone(),
            ),
        }
        .to_clvm(ctx)?)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        let mut puzzle_to_selector = HashMap::<Bytes32, u32>::new();
        let mut next_selector = 2;

        let mut puzzles = Vec::<NodePtr>::new();
        let mut selectors_and_proofs = Vec::<(u32, Option<MerkleProof>)>::new();
        let mut solutions = Vec::<NodePtr>::new();

        for (spend, proof) in solution.action_spends.into_iter().zip(solution.proofs) {
            let puzzle_hash: Bytes32 = ctx.tree_hash(spend.puzzle).into();
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

pub const DEFAULT_FINALIZER_PUZZLE: [u8; 617] = hex!("ff02ffff01ff04ffff04ff10ffff04ffff02ff12ffff04ff02ffff04ff05ffff04ffff02ff12ffff04ff02ffff04ff17ffff04ffff0bffff0101ff1780ff8080808080ffff04ffff0bffff0101ff2f80ffff04ffff02ff1effff04ff02ffff04ff82033fff80808080ff80808080808080ffff04ffff0101ffff04ffff04ff0bff8080ff8080808080ffff02ff1affff04ff02ffff04ff8201bfff8080808080ffff04ffff01ffffff3302ffff02ffff03ff05ffff01ff0bff7cffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff14ffff04ff02ffff04ff0dff80808080ff808080808080ffff016c80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffff0bff5cffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff14ffff04ff02ffff04ff07ff80808080ff808080808080ff02ffff03ff09ffff01ff04ff11ffff02ff1affff04ff02ffff04ffff04ff19ff0d80ff8080808080ffff01ff02ffff03ff0dffff01ff02ff1affff04ff02ffff04ff0dff80808080ff8080ff018080ff0180ffff0bff18ffff0bff18ff6cff0580ffff0bff18ff0bff4c8080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");
pub const DEFAULT_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    34b1f957ca3ba935921c32625cd432316ae71344977d96b4ffc5243c7d08d781
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer1stCurryArgs {
    pub action_layer_mod_hash: Bytes32,
    pub hint: Bytes32,
}

impl DefaultFinalizer1stCurryArgs {
    pub fn new(hint: Bytes32) -> Self {
        Self {
            action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
            hint,
        }
    }

    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DEFAULT_FINALIZER_PUZZLE_HASH,
            args: DefaultFinalizer1stCurryArgs::new(hint),
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer2ndCurryArgs {
    pub finalizer_self_hash: Bytes32,
}

impl DefaultFinalizer2ndCurryArgs {
    pub fn new(hint: Bytes32) -> Self {
        Self {
            finalizer_self_hash: DefaultFinalizer1stCurryArgs::curry_tree_hash(hint).into(),
        }
    }

    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        let self_hash: TreeHash = DefaultFinalizer1stCurryArgs::curry_tree_hash(hint);

        CurriedProgram {
            program: self_hash,
            args: DefaultFinalizer2ndCurryArgs {
                finalizer_self_hash: self_hash.into(),
            },
        }
        .tree_hash()
    }
}

pub const RESERVE_FINALIZER_PUZZLE: [u8; 884] = hex!("ff02ffff01ff04ffff04ff10ffff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff1affff04ff02ffff04ff81bfffff04ffff0bffff0101ff81bf80ff8080808080ffff04ffff0bffff0101ff82017f80ffff04ffff02ff3effff04ff02ffff04ff8219ffff80808080ff80808080808080ffff04ffff0101ffff04ffff04ff5fff8080ff8080808080ffff04ffff04ff18ffff04ffff0117ffff04ffff02ff3effff04ff02ffff04ffff04ffff0101ffff04ffff04ff10ffff04ff17ffff04ffff02ff2fff8219ff80ffff04ffff04ff17ff8080ff8080808080ffff06ffff02ff2effff04ff02ffff04ff820dffffff01ff80ff8080808080808080ff80808080ffff04ffff30ff8213ffff0bffff02ff2fff8202ff8080ff8080808080ffff05ffff02ff2effff04ff02ffff04ff820dffffff01ff80ff8080808080808080ffff04ffff01ffffff3342ff02ff02ffff03ff05ffff01ff0bff72ffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff1cffff04ff02ffff04ff0dff80808080ff808080808080ffff016280ff0180ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff0bff52ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff1cffff04ff02ffff04ff07ff80808080ff808080808080ffff0bff14ffff0bff14ff62ff0580ffff0bff14ff0bff428080ffff02ffff03ff09ffff01ff02ffff03ffff09ff21ffff0181d680ffff01ff02ff2effff04ff02ffff04ffff04ff19ff0d80ffff04ff0bffff04ffff04ff31ff1780ff808080808080ffff01ff02ff2effff04ff02ffff04ffff04ff19ff0d80ffff04ffff04ff11ff0b80ffff04ff17ff80808080808080ff0180ffff01ff02ffff03ff0dffff01ff02ff2effff04ff02ffff04ff0dffff04ff0bffff04ff17ff808080808080ffff01ff04ff0bff178080ff018080ff0180ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");
pub const RESERVE_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d277207ecea05d2b6a3874ef3bf5831cd224527eedab8c000a03b5511fb511de
    "
));

// run '(mod state (f state))' -d
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM: [u8; 1] = hex!("02");
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH: TreeHash =
    TreeHash::new(hex!(
        "a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"
    ));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer1stCurryArgs<P> {
    pub action_layer_mod_hash: Bytes32,
    pub reserve_full_puzzle_hash: Bytes32,
    pub reserve_inner_puzzle_hash: Bytes32,
    pub reserve_amount_from_state_program: P,
    pub hint: Bytes32,
}

impl<P> ReserveFinalizer1stCurryArgs<P> {
    pub fn new(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: P,
        hint: Bytes32,
    ) -> Self {
        Self {
            action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
            reserve_full_puzzle_hash,
            reserve_inner_puzzle_hash,
            reserve_amount_from_state_program,
            hint,
        }
    }

    pub fn curry_tree_hash(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: TreeHash,
        hint: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: RESERVE_FINALIZER_PUZZLE_HASH,
            args: ReserveFinalizer1stCurryArgs::new(
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program,
                hint,
            ),
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer2ndCurryArgs {
    pub finalizer_self_hash: Bytes32,
}

impl ReserveFinalizer2ndCurryArgs {
    pub fn new<P>(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: P,
        hint: Bytes32,
    ) -> Self
    where
        P: ToTreeHash,
    {
        Self {
            finalizer_self_hash: ReserveFinalizer1stCurryArgs::<TreeHash>::curry_tree_hash(
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program.tree_hash(),
                hint,
            )
            .into(),
        }
    }

    pub fn curry_tree_hash(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: TreeHash,
        hint: Bytes32,
    ) -> TreeHash {
        let self_hash: TreeHash = ReserveFinalizer1stCurryArgs::<TreeHash>::curry_tree_hash(
            reserve_full_puzzle_hash,
            reserve_inner_puzzle_hash,
            reserve_amount_from_state_program,
            hint,
        );

        CurriedProgram {
            program: self_hash,
            args: ReserveFinalizer2ndCurryArgs {
                finalizer_self_hash: self_hash.into(),
            },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct ReserveFinalizerSolution {
    pub reserve_parent_id: Bytes32,
}

pub const ACTION_LAYER_PUZZLE: [u8; 670] = hex!("ff02ffff01ff02ff05ffff04ff0bffff04ff17ffff04ffff02ff0affff04ff02ffff04ff2fffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ffff04ffff02ff0cffff04ff02ffff04ff0bffff04ff2fffff04ff80ffff04ff5fff80808080808080ffff04ff81bfff8080808080808080ffff04ff82017fff808080808080ffff04ffff01ffffff02ffff03ffff09ff05ff1380ffff01ff0101ffff01ff02ff08ffff04ff02ffff04ff05ffff04ff1bff808080808080ff0180ff02ffff03ff2fffff01ff02ffff03ffff02ffff03ff81cfffff01ff09ff05ffff02ff1effff04ff02ffff04ffff0bffff0101ffff02ff16ffff04ff02ffff04ffff02ff818fff0b80ff8080808080ffff04ff81cfff808080808080ffff01ff02ff08ffff04ff02ffff04ff818fffff04ff17ff808080808080ff0180ffff01ff02ff0cffff04ff02ffff04ff05ffff04ff0bffff04ffff04ff818fff1780ffff04ff6fff80808080808080ffff01ff088080ff0180ffff011780ff0180ffff02ffff03ff2fffff01ff02ff0affff04ff02ffff04ff05ffff04ffff04ff37ff0b80ffff04ffff02ffff02ff4fff0580ffff04ff27ffff04ff819fff80808080ffff04ff6fffff04ff81dfff8080808080808080ffff01ff04ff27ffff04ff37ff0b808080ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff02ffff03ff1bffff01ff02ff1effff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380ffff01ff0bffff0102ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13ffff0181ff80ff3b80ff8080808080ffff010580ff0180ff018080");
pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2ad6e558c952fb62de6428fb8d627bcd21ddf37aa8aabb43a8620d98e922a163
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ActionLayerArgs<F, S> {
    pub finalizer: F,
    pub merkle_root: Bytes32,
    pub state: S,
}

impl<F, S> ActionLayerArgs<F, S> {
    pub fn new(finalizer: F, merkle_root: Bytes32, state: S) -> Self {
        Self {
            finalizer,
            merkle_root,
            state,
        }
    }
}

impl ActionLayerArgs<TreeHash, TreeHash> {
    pub fn curry_tree_hash(
        finalizer: TreeHash,
        merkle_root: Bytes32,
        state_hash: TreeHash,
    ) -> TreeHash {
        CurriedProgram {
            program: ACTION_LAYER_PUZZLE_HASH,
            args: ActionLayerArgs::<TreeHash, TreeHash>::new(finalizer, merkle_root, state_hash),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RawActionLayerSolution<P, S, F> {
    pub puzzles: Vec<P>,
    pub selectors_and_proofs: Vec<(u32, Option<MerkleProof>)>,
    pub solutions: Vec<S>,
    pub finalizer_solution: F,
}
