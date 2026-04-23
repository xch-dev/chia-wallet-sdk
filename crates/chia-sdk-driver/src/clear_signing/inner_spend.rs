use chia_protocol::Bytes32;
use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, match_quote};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{
    ClawbackV2, DelegatedPuzzleFeederLayer, DriverError, Facts, HashedPtr, IndexWrapperLayer,
    Layer, P2OneOfManyLayer, Puzzle, RevealedP2Puzzle, SingletonMemberLayer, Spend,
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
}

#[derive(Debug, Clone)]
pub struct P2SingletonInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub conditions: Vec<Condition>,
    pub p2_puzzle_hash: Bytes32,
}

type P2SingletonLayers = IndexWrapperLayer<usize, DelegatedPuzzleFeederLayer<SingletonMemberLayer>>;

pub fn parse_inner_spend(
    facts: &Facts,
    allocator: &Allocator,
    puzzle: Puzzle,
    solution: NodePtr,
) -> Result<InnerSpend, DriverError> {
    let p2_puzzle_hash = puzzle.curried_puzzle_hash().into();

    if let Some(puzzle) = P2SingletonLayers::parse_puzzle(allocator, puzzle)? {
        let solution = P2SingletonLayers::parse_solution(allocator, solution)?;
        let delegated_spend = Spend::new(solution.delegated_puzzle, solution.delegated_solution);

        if tree_hash(allocator, delegated_spend.solution) != HashedPtr::NIL.tree_hash() {
            return Err(DriverError::InvalidDelegatedSpendFormat);
        }

        let (_, conditions) =
            <match_quote!(Vec<Condition>)>::from_clvm(allocator, delegated_spend.puzzle)?;

        Ok(InnerSpend {
            clawback: None,
            custody: Some(CustodyInfo::P2Singleton(P2SingletonInfo {
                launcher_id: puzzle.inner_puzzle.inner_puzzle.launcher_id,
                nonce: puzzle.nonce,
                conditions,
                p2_puzzle_hash,
            })),
        })
    } else if let Some(p2_puzzle) = facts.p2_puzzle(puzzle.curried_puzzle_hash().into()) {
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

                if path != ClawbackPath::PushThrough {
                    let solution_puzzle = Puzzle::parse(allocator, solution.puzzle);
                    let inner_spend =
                        parse_inner_spend(facts, allocator, solution_puzzle, solution.solution)?;

                    if inner_spend.clawback.is_some() {
                        return Err(DriverError::NestedClawback);
                    }

                    result.custody = inner_spend.custody;
                }

                Ok(result)
            }
            RevealedP2Puzzle::DelegatedConditions(conditions) => Ok(InnerSpend {
                clawback: None,
                custody: Some(CustodyInfo::DelegatedConditions(conditions.clone())),
            }),
        }
    } else {
        Ok(InnerSpend {
            clawback: None,
            custody: None,
        })
    }
}
