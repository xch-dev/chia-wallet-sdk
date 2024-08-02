use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm, ToNodePtr};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    NFTOwnershipLayer, NFTOwnershipLayerSolution, NFTStateLayer, NFTStateLayerSolution, ParseError,
    PuzzleLayer, SingletonLayer, SingletonLayerSolution, StandardLayer, StandardLayerSolution,
};

#[derive(Debug, Clone, Copy)]
pub struct NFT<M = NodePtr> {
    pub coin: Coin,

    // singleton layer
    pub launcher_id: Bytes32,

    // state layer
    pub metadata: M,

    // ownership layer
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,

    // innermost (owner) layer
    pub owner_puzzle_hash: TreeHash,
    pub owner_synthetic_key: Option<PublicKey>,
}

impl<M> NFT<M>
where
    M: ToClvm<NodePtr> + FromClvm<NodePtr>,
    SingletonLayer<NFTStateLayer<M, NFTOwnershipLayer<StandardLayer>>>: PuzzleLayer<
        SingletonLayerSolution<
            NFTStateLayerSolution<NFTOwnershipLayerSolution<StandardLayerSolution<NodePtr>>>,
        >,
    >,
{
    pub fn new(
        coin: Coin,
        launcher_id: Bytes32,
        metadata: M,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_percentage: u16,
        owner_puzzle_hash: TreeHash,
        owner_synthetic_key: Option<PublicKey>,
    ) -> Self {
        NFT {
            coin,
            launcher_id,
            metadata,
            current_owner,
            royalty_puzzle_hash,
            royalty_percentage,
            owner_puzzle_hash,
            owner_synthetic_key,
        }
    }

    pub fn with_owner_synthetic_key(mut self, owner_synthetic_key: PublicKey) -> Self {
        self.owner_synthetic_key = Some(owner_synthetic_key);
        self
    }

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: CoinSpend,
    ) -> Result<Option<Self>, ParseError> {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_node_ptr(allocator)
            .map_err(|err| ParseError::ToClvm(err))?;
        let solution_ptr = cs
            .solution
            .to_node_ptr(allocator)
            .map_err(|err| ParseError::ToClvm(err))?;

        let res = SingletonLayer::<NFTStateLayer<M, NFTOwnershipLayer<StandardLayer>>>::from_parent_spend(
            allocator,
            puzzle_ptr,
            solution_ptr,
        )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(NFT {
                coin: cs.coin,
                launcher_id: res.launcher_id,
                metadata: res.inner_puzzle.metadata,
                current_owner: res.inner_puzzle.inner_puzzle.current_owner,
                royalty_puzzle_hash: res.inner_puzzle.inner_puzzle.royalty_puzzle_hash,
                royalty_percentage: res.inner_puzzle.inner_puzzle.royalty_percentage,
                owner_puzzle_hash: res.inner_puzzle.inner_puzzle.inner_puzzle.puzzle_hash,
                owner_synthetic_key: res.inner_puzzle.inner_puzzle.inner_puzzle.synthetic_key,
            })),
        }
    }
}
