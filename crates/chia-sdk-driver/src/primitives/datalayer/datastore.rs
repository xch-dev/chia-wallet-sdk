use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::NftStateLayerSolution,
    singleton::{LauncherSolution, SingletonSolution, SINGLETON_LAUNCHER_PUZZLE_HASH},
    EveProof, LineageProof, Proof,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DelegationLayerSolution, DriverError, Layer, NftStateLayer, Puzzle, SingletonLayer, Spend,
    SpendContext,
};

use super::{get_merkle_tree, DataStoreInfo, DataStoreMetadata, DelegatedPuzzle};

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
    pub fn spend(self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<CoinSpend, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let (puzzle_ptr, solution_ptr) = if !self.info.delegated_puzzles.is_empty() {
            let layers = self.info.clone().into_layers_with_delegation_layer(ctx)?;

            let puzzle_ptr = layers.construct_puzzle(ctx)?;
            let puzzle_reveal_hash = tree_hash(ctx.allocator(), puzzle_ptr);

            let tree = get_merkle_tree(ctx, self.info.delegated_puzzles)?;

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
        } else {
            let layers = self
                .info
                .clone()
                .into_layers_without_delegation_layer(inner_spend.puzzle);

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

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DLLauncherKVList<M = DataStoreMetadata, T = NodePtr> {
    pub metadata: M,
    pub state_layer_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub memos: Vec<T>,
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct OldDLLauncherKVList<T = NodePtr> {
    pub root_hash: Bytes32,
    pub state_layer_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub memos: Vec<T>,
}

// does not Primitive because it needs extra info :(
impl DataStore<DataStoreMetadata> {
    pub fn build_datastore_info(
        allocator: &mut Allocator,
        coin: Coin,
        launcher_id: Bytes32,
        proof: Proof,
        metadata: DataStoreMetadata,
        fallback_owner_ph: Bytes32,
        memos: Vec<Bytes>,
    ) -> Result<Self, DriverError> {
        if memos.len() == 0 {
            // no hints; owner puzzle hash is the inner puzzle hash
            return Ok(DataStore {
                coin,
                proof,
                info: DataStoreInfo {
                    launcher_id,
                    metadata,
                    owner_puzzle_hash: fallback_owner_ph,
                    delegated_puzzles: vec![],
                },
            });
        }

        if memos.drain(0..1).next().ok_or(DriverError::MissingMemo)? != launcher_id.into() {
            return Err(DriverError::InvalidMemo);
        }

        if memos.len() == 2 && memos[0] == metadata.root_hash.into() {
            // vanilla store using old memo format
            return Ok(DataStore {
                coin,
                proof,
                info: DataStoreInfo {
                    launcher_id,
                    metadata,
                    owner_puzzle_hash: Bytes32::from_bytes(&memos[1])
                        .map_err(|_| ParseError::MissingHint)?,
                    delegated_puzzles: None,
                },
            });
        }

        let owner_puzzle_hash: Bytes32 = if memos.is_empty() {
            fallback_owner_ph
        } else {
            Bytes32::from_bytes(&memos.drain(0..1).next().unwrap())
                .map_err(|_| ParseError::MissingHint)?
        };

        let delegated_puzzles = {
            let mut d_puzz: Vec<DelegatedPuzzle> = vec![];

            while memos.len() > 1 {
                d_puzz.push(DelegatedPuzzle::from_memos(&mut memos)?);
            }

            Ok(d_puzz)
        }?;

        Ok(DataStore {
            coin,
            proof,
            info: DataStoreInfo {
                launcher_id,
                metadata,
                owner_puzzle_hash,
                delegated_puzzles,
            },
        })
    }

    pub fn from_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
        parent_delegated_puzzles: Option<Vec<DelegatedPuzzle>>,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let solution_node_ptr = cs
            .solution
            .to_clvm(allocator)
            .map_err(|err| DriverError::ToClvm(err))?;

        if cs.coin.puzzle_hash == SINGLETON_LAUNCHER_PUZZLE_HASH.into() {
            // we're just launching this singleton :)
            // solution is (singleton_full_puzzle_hash amount key_value_list)
            // kv_list is (metadata state_layer_hash)
            let launcher_id = cs.coin.coin_id();

            let proof = Proof::Eve(EveProof {
                parent_parent_coin_info: cs.coin.parent_coin_info,
                parent_amount: cs.coin.amount,
            });

            let solution =
                LauncherSolution::<DLLauncherKVList<DataStoreMetadata, Bytes>>::from_clvm(
                    allocator,
                    solution_node_ptr,
                );

            return match solution {
                Ok(solution) => {
                    let metadata = solution.key_value_list.metadata;

                    let new_coin = Coin {
                        parent_coin_info: launcher_id,
                        puzzle_hash: solution.singleton_puzzle_hash,
                        amount: solution.amount,
                    };

                    let mut memos: Vec<Bytes> = vec![launcher_id.into()];
                    memos.extend(solution.key_value_list.memos);

                    Self::build_datastore(
                        allocator,
                        new_coin,
                        launcher_id,
                        proof,
                        metadata,
                        solution.key_value_list.state_layer_inner_puzzle_hash,
                        &memos,
                    )?
                }
                Err(err) => match err {
                    FromClvmError::ExpectedPair => {
                        debug_log!(
              "expected pair error; datastore might've been launched using old memo format"
            ); // todo: debug
                        let solution = LauncherSolution::<OldDLLauncherKVList<Bytes>>::from_clvm(
                            allocator,
                            solution_node_ptr,
                        )?;

                        let coin = Coin {
                            parent_coin_info: launcher_id,
                            puzzle_hash: solution.singleton_puzzle_hash,
                            amount: solution.amount,
                        };

                        Ok(DataStoreInfo::build_datastore_info(
                            allocator,
                            coin,
                            launcher_id,
                            proof,
                            DataStoreMetadata::root_hash_only(solution.key_value_list.root_hash),
                            solution.key_value_list.state_layer_inner_puzzle_hash,
                            &solution.key_value_list.memos,
                        )?)
                    }
                    _ => Err(ParseError::FromClvm(err)),
                },
            };
        }

        let Some(singleton_layer) =
            SingletonLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let Some(state_layer) =
            NftStateLayer::<M, Puzzle>::parse_puzzle(allocator, singleton_layer.inner_puzzle)?
        else {
            return Ok(None);
        };

        let parent_solution =
            SingletonLayer::<NftStateLayer<M, Puzzle>>::parse_solution(allocator, parent_solution)?;

        // At this point, inner puzzle might be either a delegation layer or just an ownership layer.
        let inner_puzzle = state_layer.inner_puzzle.ptr();
        let inner_solution = parent_solution.inner_solution.inner_solution;

        let output = run_puzzle(allocator, inner_puzzle, inner_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut create_coin = None;
        let mut new_metadata = None;

        for condition in conditions {
            match condition {
                Condition::CreateCoin(condition) if condition.amount % 2 == 1 => {
                    create_coin = Some(condition);
                }
                Condition::Other(condition) => {
                    if let Ok(condition) =
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
