use chia_protocol::{Coin, CoinSpend};
use chia_puzzles::{nft::NftStateLayerSolution, singleton::SingletonSolution, LineageProof, Proof};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DelegationLayerSolution, DriverError, Layer, Primitive, Puzzle, SingletonLayer, Spend,
    SpendContext,
};

use super::{get_merkle_tree, DataStoreInfo, DataStoreMetadata};

/// Everything that is required to spend a ``DataStore`` coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStore<M = DataStoreMetadata> {
    /// The coin that holds this ``DataStore``.
    pub coin: Coin,
    /// The lineage proof for the singletonlayer.
    pub proof: Proof,
    /// The info associated with the ``DataStore``, including the metadata.
    pub info: DataStoreInfo<M>,
}

impl<M> DataStore<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub fn new(coin: Coin, proof: Proof, info: DataStoreInfo<M>) -> Self {
        DataStore { coin, proof, info }
    }

    /// Creates a coin spend for this ``DataStore``.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<CoinSpend, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let (puzzle_ptr, solution_ptr) = match self.info.delegated_puzzles {
            Some(delegated_puzzles) => {
                let layers = self.info.clone().into_layers_with_delegation_layer(ctx)?;

                let puzzle_ptr = layers.construct_puzzle(ctx)?;
                let puzzle_reveal_hash = tree_hash(ctx.allocator(), puzzle_ptr);

                let tree = get_merkle_tree(ctx, delegated_puzzles)?;

                let inner_solution = DelegationLayerSolution {
                    merkle_proof: tree.generate_proof(puzzle_reveal_hash.into()),
                    puzzle_reveal: inner_spend.puzzle,
                    puzzle_solution: inner_spend.solution,
                };

                let solution_ptr = layers.construct_solution(
                    ctx,
                    SingletonSolution {
                        lineage_proof: self.proof,
                        amount: self.coin.amount,
                        inner_solution: NftStateLayerSolution { inner_solution },
                    },
                )?;
                (puzzle_ptr, solution_ptr)
            }
            None => {
                let layers = self
                    .info
                    .clone()
                    .into_layers_without_delegation_layer(inner_spend);

                let solution_ptr = layers.construct_solution(
                    ctx,
                    SingletonSolution {
                        lineage_proof: self.proof,
                        amount: self.coin.amount,
                        inner_solution: NftStateLayerSolution {
                            inner_solution: inner_spend.solution,
                        },
                    },
                )?;

                (layers.construct_puzzle(ctx)?, solution_ptr)
            }
        };

        let puzzle = ctx.serialize(&puzzle_ptr)?;
        let solution = ctx.serialize(&solution_ptr)?;

        Ok(CoinSpend::new(self.coin, puzzle, solution))
    }

    /// Returns the lineage proof that would be used by the child.
    pub fn child_lineage_proof(&self, ctx: &mut SpendContext) -> Result<LineageProof, DriverError>
    where
        M: ToTreeHash,
    {
        Ok(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash(ctx)?.into(),
            parent_amount: self.coin.amount,
        })
    }
}

// todo: -----------------------------------\/--------------------------------------

impl<M> Primitive for DataStore<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(singleton_layer) =
            SingletonLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let Some(inner_layers) =
            NftStateLayer::<M, NftOwnershipLayer<RoyaltyTransferLayer, Puzzle>>::parse_puzzle(
                allocator,
                singleton_layer.inner_puzzle,
            )?
        else {
            return Ok(None);
        };

        let parent_solution = SingletonLayer::<
            NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, Puzzle>>,
        >::parse_solution(allocator, parent_solution)?;

        let inner_puzzle = inner_layers.inner_puzzle.inner_puzzle;
        let inner_solution = parent_solution.inner_solution.inner_solution.inner_solution;

        let output = run_puzzle(allocator, inner_puzzle.ptr(), inner_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut create_coin = None;
        let mut new_owner = None;
        let mut new_metadata = None;

        for condition in conditions {
            match condition {
                Condition::CreateCoin(condition) if condition.amount % 2 == 1 => {
                    create_coin = Some(condition);
                }
                Condition::Other(condition) => {
                    if let Ok(condition) = NewNftOwner::from_clvm(allocator, condition) {
                        new_owner = Some(condition);
                    } else if let Ok(condition) =
                        NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition)
                    {
                        new_metadata = Some(condition);
                    }
                }
                _ => {}
            }
        }

        let Some(create_coin) = create_coin else {
            return Err(DriverError::MissingChild);
        };

        let mut layers = SingletonLayer::new(singleton_layer.launcher_id, inner_layers);

        if let Some(new_owner) = new_owner {
            layers.inner_puzzle.inner_puzzle.current_owner = new_owner.did_id;
        }

        if let Some(new_metadata) = new_metadata {
            let output = run_puzzle(
                allocator,
                new_metadata.metadata_updater_reveal,
                new_metadata.metadata_updater_solution,
            )?;

            let output =
                NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?.metadata_part;
            layers.inner_puzzle.metadata = output.new_metadata;
            layers.inner_puzzle.metadata_updater_puzzle_hash = output.new_metadata_updater_puzhash;
        }

        let mut info = NftInfo::from_layers(layers);
        info.p2_puzzle_hash = create_coin.puzzle_hash;

        Ok(Some(Self {
            coin,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: singleton_layer.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
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
    .to_clvm(&mut allocator)
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(nft_full_puzzle_hash);
    hasher.update([0xad, 0x4c]);
    hasher.update(tree_hash(&allocator, new_nft_owner_args));

    Bytes32::new(hasher.finalize())
}
