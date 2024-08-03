use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, Proof};
use chia_sdk_types::conditions::{Condition, CreateCoin, NewNftOwner};
use clvm_traits::{clvm_list, FromClvm, FromNodePtr, ToClvm, ToNodePtr};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{
    sha2::{Digest, Sha256},
    Allocator, NodePtr,
};

use crate::{
    Conditions, NFTOwnershipLayer, NFTOwnershipLayerSolution, NFTStateLayer, NFTStateLayerSolution,
    ParseError, PuzzleLayer, SingletonLayer, SingletonLayerSolution, SpendContext, SpendError,
    StandardLayer, StandardLayerSolution,
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

    pub fn from_puzzle(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
    ) -> Result<Option<Self>, ParseError> {
        let res =
            SingletonLayer::<NFTStateLayer<M, NFTOwnershipLayer<StandardLayer>>>::from_puzzle(
                allocator, puzzle,
            )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(NFT {
                coin,
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

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        innermost_conditions: Vec<Condition<NodePtr>>,
    ) -> Result<CoinSpend, ParseError>
    where
        M: Clone,
    {
        let thing = SingletonLayer::<NFTStateLayer<M, NFTOwnershipLayer<StandardLayer>>> {
            launcher_id: self.launcher_id,
            inner_puzzle: NFTStateLayer {
                metadata: self.metadata.clone(),
                metadata_updater_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH.into(),
                inner_puzzle: NFTOwnershipLayer {
                    launcher_id: self.launcher_id,
                    current_owner: self.current_owner,
                    royalty_puzzle_hash: self.royalty_puzzle_hash,
                    royalty_percentage: self.royalty_percentage,
                    inner_puzzle: StandardLayer {
                        puzzle_hash: self.owner_puzzle_hash,
                        synthetic_key: self.owner_synthetic_key,
                    },
                },
            },
        };

        let puzzle_ptr = thing.construct_puzzle(ctx)?;
        let puzzle = Program::from_node_ptr(ctx.allocator(), puzzle_ptr)
            .map_err(|err| ParseError::FromClvm(err))?;

        let solution_ptr = thing.construct_solution(
            ctx,
            SingletonLayerSolution {
                lineage_proof: lineage_proof,
                amount: self.coin.amount,
                inner_solution: NFTStateLayerSolution {
                    inner_solution: NFTOwnershipLayerSolution {
                        inner_solution: StandardLayerSolution {
                            conditions: innermost_conditions,
                        },
                    },
                },
            },
        )?;
        let solution = Program::from_node_ptr(ctx.allocator(), solution_ptr)
            .map_err(|err| ParseError::FromClvm(err))?;

        Ok(CoinSpend {
            coin: self.coin,
            puzzle_reveal: puzzle,
            solution,
        })
    }

    pub fn transfer(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        new_owner_puzzle_hash: Bytes32,
    ) -> Result<CoinSpend, ParseError>
    where
        M: Clone,
    {
        let p2_conditions = vec![Condition::CreateCoin(CreateCoin::with_hint(
            new_owner_puzzle_hash,
            self.coin.amount,
            new_owner_puzzle_hash,
        ))];
        self.spend(ctx, lineage_proof, p2_conditions)
    }

    pub fn transfer_to_did(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        new_owner_puzzle_hash: Bytes32,
        new_did_owner: NewNftOwner,
    ) -> Result<(CoinSpend, Conditions), ParseError>
    // (spend, did conditions)
    where
        M: Clone,
    {
        let p2_conditions = vec![
            Condition::CreateCoin(CreateCoin::with_hint(
                new_owner_puzzle_hash,
                self.coin.amount,
                new_owner_puzzle_hash,
            )),
            Condition::Other(ctx.alloc(&new_did_owner)?),
        ];

        let did_conditions = Conditions::new().assert_raw_puzzle_announcement(
            did_puzzle_assertion(self.coin.puzzle_hash, &new_did_owner),
        );

        Ok((
            self.spend(ctx, lineage_proof, p2_conditions)?,
            did_conditions,
        ))
    }
}

#[allow(clippy::missing_panics_doc)]
pub fn did_puzzle_assertion(nft_full_puzzle_hash: Bytes32, new_nft_owner: &NewNftOwner) -> Bytes32 {
    let mut allocator = Allocator::new();

    let new_nft_owner_args = clvm_list!(
        new_nft_owner.did_id,
        &new_nft_owner.trade_prices,
        new_nft_owner.did_inner_puzzle_hash
    )
    .to_node_ptr(&mut allocator)
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(nft_full_puzzle_hash);
    hasher.update([0xad, 0x4c]);
    hasher.update(tree_hash(&allocator, new_nft_owner_args));

    Bytes32::new(hasher.finalize().into())
}
