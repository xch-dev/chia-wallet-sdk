use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_sdk_types::{BinaryTree, MerkleTree, HASH_TREE_PREFIX};
use clvm_traits::{clvm_tuple, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, SpendContext};

#[derive(Debug, Clone)]
pub enum PartialTreeLeaf<T> {
    Hash(Bytes32),
    Reveal(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialMerkleTreeReveal {}

impl PartialMerkleTreeReveal {
    pub fn for_action_layer_solution(
        ctx: &mut SpendContext,
        all_puzzles: &[Bytes32],
        puzzles_to_run: &[Bytes32],
    ) -> Result<NodePtr, DriverError> {
        let mut leaf_reveals = HashMap::new();
        let mut next_selector: u32 = 2;
        for puzzle in puzzles_to_run {
            leaf_reveals.insert(*puzzle, next_selector);
            next_selector = next_selector * 2 + 1;
        }

        Self::build(ctx, all_puzzles, &leaf_reveals)
    }

    pub fn build<T>(
        ctx: &mut SpendContext,
        leaves: &[Bytes32],
        leaf_reveals: &HashMap<Bytes32, T>,
    ) -> Result<NodePtr, DriverError>
    where
        T: ToClvm<Allocator> + Clone,
    {
        let binary_tree = MerkleTree::list_to_binary_tree(leaves);

        let partial_tree = Self::convert_to_partial_tree(&binary_tree, leaf_reveals);
        let partial_tree = Self::optimize_partial_tree(&partial_tree);

        Self::optimized_tree_to_clvm(ctx, &partial_tree)
    }

    pub fn convert_to_partial_tree<T>(
        tree: &BinaryTree<Bytes32>,
        leaf_reveals: &HashMap<Bytes32, T>,
    ) -> BinaryTree<PartialTreeLeaf<T>>
    where
        T: Clone,
    {
        match tree {
            BinaryTree::Leaf(leaf) => {
                if let Some(reveal) = leaf_reveals.get(leaf) {
                    BinaryTree::Leaf(PartialTreeLeaf::Reveal(reveal.clone()))
                } else {
                    let leaf: Bytes32 = leaf.tree_hash().into();
                    BinaryTree::Leaf(PartialTreeLeaf::Hash(leaf))
                }
            }
            BinaryTree::Node(left, right) => {
                let left = Self::convert_to_partial_tree(left, leaf_reveals);
                let right = Self::convert_to_partial_tree(right, leaf_reveals);
                BinaryTree::Node(Box::new(left), Box::new(right))
            }
        }
    }

    pub fn optimize_partial_tree<T>(
        tree: &BinaryTree<PartialTreeLeaf<T>>,
    ) -> BinaryTree<PartialTreeLeaf<T>>
    where
        T: Clone,
    {
        match tree {
            BinaryTree::Leaf(leaf) => BinaryTree::Leaf(leaf.clone()),
            BinaryTree::Node(left, right) => {
                let left = Self::optimize_partial_tree(left);
                let right = Self::optimize_partial_tree(right);

                if let (
                    BinaryTree::Leaf(PartialTreeLeaf::Hash(left_reveal)),
                    BinaryTree::Leaf(PartialTreeLeaf::Hash(right_reveal)),
                ) = (left.clone(), right.clone())
                {
                    println!("hashing: 2 {:?} {:?}", left_reveal, right_reveal); // todo: debug
                    let hash = MerkleTree::sha256(&[HASH_TREE_PREFIX, &left_reveal, &right_reveal]);
                    println!("hash: {:?}", hash); // todo: debug
                    BinaryTree::Leaf(PartialTreeLeaf::Hash(hash))
                } else {
                    BinaryTree::Node(Box::new(left), Box::new(right))
                }
            }
        }
    }

    pub fn optimized_tree_to_clvm<T>(
        ctx: &mut SpendContext,
        tree: &BinaryTree<PartialTreeLeaf<T>>,
    ) -> Result<NodePtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        Self::optimized_tree_to_clvm_recursive(ctx, tree)
    }

    fn optimized_tree_to_clvm_recursive<T>(
        ctx: &mut SpendContext,
        tree: &BinaryTree<PartialTreeLeaf<T>>,
    ) -> Result<NodePtr, DriverError>
    where
        T: ToClvm<Allocator>,
    {
        match tree {
            BinaryTree::Leaf(leaf) => match leaf {
                PartialTreeLeaf::Hash(hash) => ctx.alloc(hash),
                PartialTreeLeaf::Reveal(reveal) => ctx.alloc(&clvm_tuple!((), reveal)),
            },
            BinaryTree::Node(left, right) => {
                let left = Self::optimized_tree_to_clvm_recursive(ctx, left)?;
                let right = Self::optimized_tree_to_clvm_recursive(ctx, right)?;
                ctx.alloc(&clvm_tuple!(left, right))
            }
        }
    }
}
