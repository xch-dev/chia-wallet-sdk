use chia_protocol::Bytes32;
use chia_sdk_types::{
    Condition,
    puzzles::{DelegatedPuzzleFeederSolution, OneOfNSolution, SingletonMemberSolution},
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{
    AugmentedConditionLayer, ClawbackV2, DelegatedPuzzleFeederLayer, DriverError,
    IndexWrapperLayer, Layer, P2OneOfManyLayer, Puzzle, RevealedP2Puzzle, Reveals,
    SingletonMemberLayer, Spend, parse_delegated_spend,
};

#[derive(Debug, Clone)]
pub struct InnerSpend {
    pub clawback: Option<ClawbackInfo>,
    pub custody: Option<CustodyInfo>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClawbackInfo {
    pub clawback: ClawbackV2,
    pub path: ClawbackPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClawbackPath {
    Sender,
    Receiver,
    PushThrough,
}

#[derive(Debug, Clone)]
pub enum CustodyInfo {
    P2Singleton(P2SingletonInfo),
    DelegatedConditions(Vec<Condition>),
    P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo),
}

#[derive(Debug, Clone)]
pub struct P2SingletonInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub conditions: Vec<Condition>,
    pub p2_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone)]
pub struct P2ConditionsOrSingletonInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub conditions: Vec<Condition>,
    pub p2_puzzle_hash: Bytes32,
}

type P2SingletonLayers = IndexWrapperLayer<usize, DelegatedPuzzleFeederLayer<SingletonMemberLayer>>;

pub fn parse_inner_spend(
    reveals: &Reveals,
    allocator: &Allocator,
    puzzle: Puzzle,
    solution: NodePtr,
) -> Result<InnerSpend, DriverError> {
    let p2_puzzle_hash = puzzle.curried_puzzle_hash().into();

    if let Some(puzzle) = P2SingletonLayers::parse_puzzle(allocator, puzzle)? {
        let solution = P2SingletonLayers::parse_solution(allocator, solution)?;
        let delegated_spend = Spend::new(solution.delegated_puzzle, solution.delegated_solution);
        let conditions = parse_delegated_spend(allocator, delegated_spend)?;

        Ok(InnerSpend {
            clawback: None,
            custody: Some(CustodyInfo::P2Singleton(P2SingletonInfo {
                launcher_id: puzzle.inner_puzzle.inner_puzzle.launcher_id,
                nonce: puzzle.nonce,
                conditions,
                p2_puzzle_hash,
            })),
        })
    } else if let Some(p2_puzzle) = reveals.p2_puzzle(puzzle.curried_puzzle_hash()) {
        match p2_puzzle {
            RevealedP2Puzzle::Clawback(clawback) => {
                let solution = P2OneOfManyLayer::parse_solution(allocator, solution)?;
                let merkle_tree = clawback.merkle_tree();

                let mut spent_leaf = None;

                for leaf in merkle_tree.leaves() {
                    let proof = merkle_tree
                        .proof(leaf)
                        .expect("merkle tree proof should exist");

                    if solution.merkle_proof == proof {
                        spent_leaf = Some(leaf);
                        break;
                    }
                }

                let Some(spent_leaf) = spent_leaf else {
                    return Err(DriverError::InvalidMerkleProof);
                };

                let path = if spent_leaf == clawback.sender_path_hash() {
                    ClawbackPath::Sender
                } else if spent_leaf == clawback.receiver_path_hash() {
                    ClawbackPath::Receiver
                } else {
                    ClawbackPath::PushThrough
                };

                let mut result = InnerSpend {
                    clawback: Some(ClawbackInfo {
                        clawback: *clawback,
                        path,
                    }),
                    custody: None,
                };

                let solution_puzzle = Puzzle::parse(allocator, solution.puzzle);

                if path != ClawbackPath::PushThrough
                    && let Some(augmented_condition_puzzle) =
                        AugmentedConditionLayer::<NodePtr, Puzzle>::parse_puzzle(
                            allocator,
                            solution_puzzle,
                        )?
                {
                    let augmented_condition_solution =
                        AugmentedConditionLayer::<NodePtr, Puzzle>::parse_solution(
                            allocator,
                            solution.solution,
                        )?;

                    let inner_spend = parse_inner_spend(
                        reveals,
                        allocator,
                        augmented_condition_puzzle.inner_puzzle,
                        augmented_condition_solution.inner_solution,
                    )?;

                    if inner_spend.clawback.is_some() {
                        return Err(DriverError::NestedClawback);
                    }

                    result.custody = inner_spend.custody;
                }

                Ok(result)
            }
            RevealedP2Puzzle::P2ConditionsOrSingleton(info) => {
                let solution = DelegatedPuzzleFeederSolution::<
                    NodePtr,
                    NodePtr,
                    OneOfNSolution<NodePtr, SingletonMemberSolution>,
                >::from_clvm(allocator, solution)?;

                let delegated_spend =
                    Spend::new(solution.delegated_puzzle, solution.delegated_solution);
                let conditions = parse_delegated_spend(allocator, delegated_spend)?;

                Ok(InnerSpend {
                    clawback: None,
                    custody: Some(CustodyInfo::P2ConditionsOrSingleton(
                        P2ConditionsOrSingletonInfo {
                            launcher_id: info.launcher_id,
                            nonce: info.nonce,
                            conditions,
                            p2_puzzle_hash,
                        },
                    )),
                })
            }
        }
    } else if let Ok(conditions) =
        parse_delegated_spend(allocator, Spend::new(puzzle.ptr(), solution))
    {
        Ok(InnerSpend {
            clawback: None,
            custody: Some(CustodyInfo::DelegatedConditions(conditions)),
        })
    } else {
        Ok(InnerSpend {
            clawback: None,
            custody: None,
        })
    }
}
