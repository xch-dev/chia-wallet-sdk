use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftStateLayerArgs, NftStateLayerSolution, NFT_STATE_LAYER_PUZZLE_HASH},
    singleton::{
        LauncherSolution, SingletonArgs, SingletonSolution, SINGLETON_LAUNCHER_PUZZLE_HASH,
    },
    EveProof, LineageProof, Proof,
};
use chia_sdk_types::{run_puzzle, CreateCoin};
use chia_sdk_types::{Condition, NewMetadataCondition};
use clvm_traits::{FromClvm, FromClvmError, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

use crate::{
    DelegationLayerArgs, DelegationLayerSolution, DriverError, Layer, NftStateLayer, Puzzle,
    SingletonLayer, Spend, SpendContext, DELEGATION_LAYER_PUZZLE_HASH,
};

use super::{get_merkle_tree, DataStoreInfo, DataStoreMetadata, DelegatedPuzzle, HintType};

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
        let (puzzle_ptr, solution_ptr) = if self.info.delegated_puzzles.is_empty() {
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
        } else {
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

pub trait MetadataWithRootHash {
    fn root_hash(&self) -> Bytes32;
    fn root_hash_only(root_hash: Bytes32) -> Self;
}

impl MetadataWithRootHash for DataStoreMetadata {
    fn root_hash(&self) -> Bytes32 {
        self.root_hash
    }

    fn root_hash_only(root_hash: Bytes32) -> Self {
        super::DataStoreMetadata::root_hash_only(root_hash)
    }
}

// does not Primitive because it needs extra info :(
impl<M> DataStore<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash + MetadataWithRootHash,
{
    pub fn build_datastore(
        coin: Coin,
        launcher_id: Bytes32,
        proof: Proof,
        metadata: M,
        fallback_owner_ph: Bytes32,
        memos: Vec<Bytes>,
    ) -> Result<Self, DriverError> {
        let mut memos = memos;

        if memos.is_empty() {
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

        if memos.len() == 2 && memos[0] == metadata.root_hash().into() {
            // vanilla store using old memo format
            let owner_puzzle_hash = Bytes32::new(
                memos[1]
                    .to_vec()
                    .try_into()
                    .map_err(|_| DriverError::InvalidMemo)?,
            );
            return Ok(DataStore {
                coin,
                proof,
                info: DataStoreInfo {
                    launcher_id,
                    metadata,
                    owner_puzzle_hash,
                    delegated_puzzles: vec![],
                },
            });
        }

        let owner_puzzle_hash: Bytes32 = if memos.is_empty() {
            fallback_owner_ph
        } else {
            Bytes32::new(
                memos
                    .drain(0..1)
                    .next()
                    .ok_or(DriverError::MissingMemo)?
                    .to_vec()
                    .try_into()
                    .map_err(|_| DriverError::InvalidMemo)?,
            )
        };

        let mut delegated_puzzles = vec![];
        while memos.len() > 1 {
            delegated_puzzles.push(DelegatedPuzzle::from_memos(&mut memos)?);
        }

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
        parent_delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let solution_node_ptr = cs
            .solution
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;

        if cs.coin.puzzle_hash == SINGLETON_LAUNCHER_PUZZLE_HASH.into() {
            // we're just launching this singleton :)
            // solution is (singleton_full_puzzle_hash amount key_value_list)
            // kv_list is (metadata state_layer_hash)
            let launcher_id = cs.coin.coin_id();

            let proof = Proof::Eve(EveProof {
                parent_parent_coin_info: cs.coin.parent_coin_info,
                parent_amount: cs.coin.amount,
            });

            let solution = LauncherSolution::<DLLauncherKVList<M, Bytes>>::from_clvm(
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

                    Ok(Some(Self::build_datastore(
                        new_coin,
                        launcher_id,
                        proof,
                        metadata,
                        solution.key_value_list.state_layer_inner_puzzle_hash,
                        memos,
                    )?))
                }
                Err(err) => match err {
                    FromClvmError::ExpectedPair => {
                        // datastore launched using old memo format
                        let solution = LauncherSolution::<OldDLLauncherKVList<Bytes>>::from_clvm(
                            allocator,
                            solution_node_ptr,
                        )?;

                        let coin = Coin {
                            parent_coin_info: launcher_id,
                            puzzle_hash: solution.singleton_puzzle_hash,
                            amount: solution.amount,
                        };

                        Ok(Some(Self::build_datastore(
                            coin,
                            launcher_id,
                            proof,
                            M::root_hash_only(solution.key_value_list.root_hash),
                            solution.key_value_list.state_layer_inner_puzzle_hash,
                            solution.key_value_list.memos,
                        )?))
                    }
                    _ => Err(DriverError::FromClvm(err)),
                },
            };
        }

        let parent_puzzle_ptr = cs
            .puzzle_reveal
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;
        let parent_puzzle = Puzzle::parse(allocator, parent_puzzle_ptr);

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

        let parent_solution_ptr = cs.solution.to_clvm(allocator)?;
        let parent_solution = SingletonLayer::<NftStateLayer<M, Puzzle>>::parse_solution(
            allocator,
            parent_solution_ptr,
        )?;

        // At this point, inner puzzle might be either a delegation layer or just an ownership layer.
        let inner_puzzle = state_layer.inner_puzzle.ptr();
        let inner_solution = parent_solution.inner_solution.inner_solution;

        let inner_output = run_puzzle(allocator, inner_puzzle, inner_solution)?;
        let inner_conditions = Vec::<Condition>::from_clvm(allocator, inner_output)?;

        let mut inner_create_coin_condition = None;
        let mut inner_new_metadata_condition = None;

        for condition in inner_conditions {
            match condition {
                Condition::CreateCoin(condition) if condition.amount % 2 == 1 => {
                    inner_create_coin_condition = Some(condition);
                }
                Condition::Other(condition) => {
                    if let Ok(condition) =
                        NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition)
                    {
                        inner_new_metadata_condition = Some(condition);
                    }
                }
                _ => {}
            }
        }

        let Some(inner_create_coin_condition) = inner_create_coin_condition else {
            return Err(DriverError::MissingChild);
        };

        let new_metadata = if let Some(inner_new_metadata_condition) = inner_new_metadata_condition
        {
            NftStateLayer::<M, ()>::get_next_metadata(allocator, inner_new_metadata_condition)?
        } else {
            state_layer.metadata
        };

        // first, just compute new coin info - will be used in any case

        let new_puzzle_hash = SingletonArgs::curry_tree_hash(
            singleton_layer.launcher_id,
            CurriedProgram {
                program: NFT_STATE_LAYER_PUZZLE_HASH,
                args: NftStateLayerArgs::<TreeHash, TreeHash> {
                    mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                    metadata: new_metadata.tree_hash(),
                    metadata_updater_puzzle_hash: state_layer.metadata_updater_puzzle_hash,
                    inner_puzzle: inner_create_coin_condition.puzzle_hash.into(),
                },
            }
            .tree_hash(),
        );

        let new_coin = Coin {
            parent_coin_info: cs.coin.coin_id(),
            puzzle_hash: new_puzzle_hash.into(),
            amount: inner_create_coin_condition.amount,
        };

        // if the coin was re-created with memos, there is a delegation layer
        // and delegated puzzles have been updated (we can rebuild the list from memos)
        if inner_create_coin_condition.memos.len() > 1 {
            // keep in mind that there's always the launcher id memo being added
            return Ok(Some(Self::build_datastore(
                new_coin,
                singleton_layer.launcher_id,
                Proof::Lineage(singleton_layer.lineage_proof(cs.coin)),
                new_metadata,
                state_layer.inner_puzzle.tree_hash().into(),
                inner_create_coin_condition.memos,
            )?));
        }

        let mut owner_puzzle_hash: Bytes32 = state_layer.inner_puzzle.tree_hash().into();

        // does the coin currently have a delegation layer?
        let delegation_layer_maybe = state_layer.inner_puzzle;
        if delegation_layer_maybe.is_curried()
            && delegation_layer_maybe.mod_hash() == DELEGATION_LAYER_PUZZLE_HASH
        {
            let deleg_puzzle_args = DelegationLayerArgs::from_clvm(
                allocator,
                delegation_layer_maybe
                    .as_curried()
                    .ok_or(DriverError::NonStandardLayer)?
                    .args,
            )
            .map_err(DriverError::FromClvm)?;
            owner_puzzle_hash = deleg_puzzle_args.owner_puzzle_hash;

            let delegation_layer_solution =
                DelegationLayerSolution::<NodePtr, NodePtr>::from_clvm(allocator, inner_solution)?;

            // to get more info, we'll need to run the delegated puzzle (delegation layer's "inner" puzzle)
            let output = run_puzzle(
                allocator,
                delegation_layer_solution.puzzle_reveal,
                delegation_layer_solution.puzzle_solution,
            )?;

            let odd_create_coin = Vec::<NodePtr>::from_clvm(allocator, output)?
                .iter()
                .map(|cond| Condition::<NodePtr>::from_clvm(allocator, *cond))
                .find(|cond| match cond {
                    Ok(Condition::CreateCoin(create_coin)) => create_coin.amount % 2 == 1,
                    _ => false,
                });

            let Some(odd_create_coin) = odd_create_coin else {
                // no CREATE_COIN was created by the innermost puzzle
                // delegation layer therefore added one (assuming the spend is valid)
                return Ok(Some(DataStore {
                    coin: new_coin,
                    proof: Proof::Lineage(singleton_layer.lineage_proof(cs.coin)),
                    info: DataStoreInfo {
                        launcher_id: singleton_layer.launcher_id,
                        metadata: new_metadata,
                        owner_puzzle_hash,
                        delegated_puzzles: parent_delegated_puzzles,
                    },
                }));
            };

            let odd_create_coin = odd_create_coin?;

            // if there were any memos, the if above would have caught it since it processes
            // output conditions of the state layer inner puzzle (i.e., it runs the delegation layer)
            // therefore, this spend is either 'exiting' the delegation layer or re-creatign it
            if let Condition::CreateCoin(create_coin) = odd_create_coin {
                let prev_deleg_layer_ph = delegation_layer_maybe.tree_hash();

                if create_coin.puzzle_hash == prev_deleg_layer_ph.into() {
                    // owner is re-creating the delegation layer with the same options
                    return Ok(Some(DataStore {
                        coin: new_coin,
                        proof: Proof::Lineage(singleton_layer.lineage_proof(cs.coin)),
                        info: DataStoreInfo {
                            launcher_id: singleton_layer.launcher_id,
                            metadata: new_metadata,
                            owner_puzzle_hash, // owner puzzle was ran
                            delegated_puzzles: parent_delegated_puzzles,
                        },
                    }));
                }

                // owner is exiting the delegation layer
                owner_puzzle_hash = create_coin.puzzle_hash;
            }
        }

        // all methods exhausted; this coin doesn't seem to have a delegation layer
        Ok(Some(DataStore {
            coin: new_coin,
            proof: Proof::Lineage(singleton_layer.lineage_proof(cs.coin)),
            info: DataStoreInfo {
                launcher_id: singleton_layer.launcher_id,
                metadata: new_metadata,
                owner_puzzle_hash,
                delegated_puzzles: vec![],
            },
        }))
    }
}

impl<M> DataStore<M> {
    pub fn get_recreation_memos(
        launcher_id: Bytes32,
        owner_puzzle_hash: TreeHash,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Vec<Bytes> {
        let owner_puzzle_hash: Bytes32 = owner_puzzle_hash.into();
        let mut memos: Vec<Bytes> = vec![launcher_id.into(), owner_puzzle_hash.into()];

        for delegated_puzzle in delegated_puzzles {
            match delegated_puzzle {
                DelegatedPuzzle::Admin(inner_puzzle_hash) => {
                    memos.push(Bytes::new([HintType::AdminPuzzle.value()].into()));
                    memos.push(inner_puzzle_hash.into());
                }
                DelegatedPuzzle::Writer(inner_puzzle_hash) => {
                    memos.push(Bytes::new([HintType::WriterPuzzle.value()].into()));
                    memos.push(inner_puzzle_hash.into());
                }
                DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                    memos.push(Bytes::new([HintType::OraclePuzzle.value()].into()));
                    memos.push(oracle_puzzle_hash.into());

                    let fee_bytes = BigInt::from(oracle_fee).to_signed_bytes_be();
                    let mut fee_bytes = fee_bytes.as_slice();

                    // https://github.com/Chia-Network/clvm_rs/blob/66a17f9576d26011321bb4c8c16eb1c63b169f1f/src/allocator.rs#L295
                    while (!fee_bytes.is_empty()) && (fee_bytes[0] == 0) {
                        if fee_bytes.len() > 1 && (fee_bytes[1] & 0x80 == 0x80) {
                            break;
                        }
                        fee_bytes = &fee_bytes[1..];
                    }

                    memos.push(fee_bytes.into());
                }
            }
        }

        memos
    }

    // As an owner use CREATE_COIN to:
    //  - just re-create store (no hints needed)
    //  - change delegated puzzles (hints needed)
    pub fn owner_create_coin_condition(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        new_inner_puzzle_hash: Bytes32,
        new_delegated_puzzles: Vec<DelegatedPuzzle>,
        hint_delegated_puzzles: bool,
    ) -> Result<Condition, DriverError> {
        let new_puzzle_hash = if new_delegated_puzzles.is_empty() {
            let new_merkle_root = get_merkle_tree(ctx, new_delegated_puzzles.clone())?.get_root();
            DelegationLayerArgs::curry_tree_hash(
                launcher_id,
                new_inner_puzzle_hash,
                new_merkle_root,
            )
            .into()
        } else {
            new_inner_puzzle_hash
        };

        Ok(Condition::CreateCoin(CreateCoin {
            amount: 1,
            puzzle_hash: new_puzzle_hash,
            memos: if hint_delegated_puzzles {
                Self::get_recreation_memos(
                    launcher_id,
                    new_inner_puzzle_hash.into(),
                    new_delegated_puzzles,
                )
            } else {
                vec![launcher_id.into()]
            },
        }))
    }
}
