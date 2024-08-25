use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftStateLayerArgs, NftStateLayerSolution, NFT_STATE_LAYER_PUZZLE_HASH},
    singleton::{
        LauncherSolution, SingletonArgs, SingletonSolution, SINGLETON_LAUNCHER_PUZZLE_HASH,
    },
    EveProof, LineageProof, Proof,
};
use chia_sdk_types::{run_puzzle, CreateCoin, NewMetadataInfo, NewMetadataOutput};
use chia_sdk_types::{Condition, NewMetadataCondition};
use clvm_traits::{FromClvm, FromClvmError, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

use crate::{
    DelegationLayerArgs, DelegationLayerSolution, DriverError, Layer, NftStateLayer, Puzzle,
    SingletonLayer, Spend, SpendContext, DELEGATION_LAYER_PUZZLE_HASH,
    DL_METADATA_UPDATER_PUZZLE_HASH,
};

use super::{
    get_merkle_tree, DataStoreInfo, DataStoreMetadata, DelegatedPuzzle, HintType,
    MetadataWithRootHash,
};

/// Everything that is required to spend a [`DataStore`] coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStore<M = DataStoreMetadata> {
    /// The coin that holds this [`DataStore`].
    pub coin: Coin,
    /// The lineage proof for the singletonlayer.
    pub proof: Proof,
    /// The info associated with the [`DataStore`], including the metadata.
    pub info: DataStoreInfo<M>,
}

impl<M> DataStore<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub fn new(coin: Coin, proof: Proof, info: DataStoreInfo<M>) -> Self {
        DataStore { coin, proof, info }
    }

    /// Creates a coin spend for this [`DataStore`].
    pub fn spend(self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<CoinSpend, DriverError>
    where
        M: Clone,
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

            let delegated_puzzle_hash = tree_hash(&ctx.allocator, inner_spend.puzzle);

            let tree = get_merkle_tree(ctx, self.info.delegated_puzzles)?;

            let inner_solution = DelegationLayerSolution {
                // if running owner puzzle, the line below will return 'None', thus ensuring correct puzzle behavior
                merkle_proof: tree.get_proof(delegated_puzzle_hash.into()),
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
    pub fn child_lineage_proof(&self, ctx: &mut SpendContext) -> Result<LineageProof, DriverError> {
        Ok(LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash(ctx)?.into(),
            parent_amount: self.coin.amount,
        })
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DlLauncherKvList<M = DataStoreMetadata, T = NodePtr> {
    pub metadata: M,
    pub state_layer_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub memos: Vec<T>,
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct OldDlLauncherKvList<T = NodePtr> {
    pub root_hash: Bytes32,
    pub state_layer_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub memos: Vec<T>,
}

// Does not implement Primitive because it needs extra info.
impl<M> DataStore<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + MetadataWithRootHash,
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
        parent_delegated_puzzles: &[DelegatedPuzzle],
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

            let solution = LauncherSolution::<DlLauncherKvList<M, Bytes>>::from_clvm(
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
                        let solution = LauncherSolution::<OldDlLauncherKvList<Bytes>>::from_clvm(
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
            NftStateLayer::<M, NodePtr>::get_next_metadata(
                allocator,
                &state_layer.metadata,
                state_layer.metadata_updater_puzzle_hash,
                inner_new_metadata_condition,
            )?
        } else {
            state_layer.metadata
        };

        // first, just compute new coin info - will be used in any case

        let new_metadata_ptr = new_metadata.to_clvm(allocator)?;
        let new_puzzle_hash = SingletonArgs::curry_tree_hash(
            singleton_layer.launcher_id,
            CurriedProgram {
                program: NFT_STATE_LAYER_PUZZLE_HASH,
                args: NftStateLayerArgs::<TreeHash, TreeHash> {
                    mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                    metadata: tree_hash(allocator, new_metadata_ptr),
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

        // does the parent coin currently have a delegation layer?
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
                // delegation layer therefore added one (assuming the spend is valid)]
                return Ok(Some(DataStore {
                    coin: new_coin,
                    proof: Proof::Lineage(singleton_layer.lineage_proof(cs.coin)),
                    info: DataStoreInfo {
                        launcher_id: singleton_layer.launcher_id,
                        metadata: new_metadata,
                        owner_puzzle_hash,
                        delegated_puzzles: parent_delegated_puzzles.to_vec(),
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
                            delegated_puzzles: parent_delegated_puzzles.to_vec(),
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
                    memos.push(Bytes::new([HintType::AdminPuzzle as u8].into()));
                    memos.push(Bytes32::from(inner_puzzle_hash).into());
                }
                DelegatedPuzzle::Writer(inner_puzzle_hash) => {
                    memos.push(Bytes::new([HintType::WriterPuzzle as u8].into()));
                    memos.push(Bytes32::from(inner_puzzle_hash).into());
                }
                DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                    memos.push(Bytes::new([HintType::OraclePuzzle as u8].into()));
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
            new_inner_puzzle_hash
        } else {
            let new_merkle_root = get_merkle_tree(ctx, new_delegated_puzzles.clone())?.root;
            DelegationLayerArgs::curry_tree_hash(
                launcher_id,
                new_inner_puzzle_hash,
                new_merkle_root,
            )
            .into()
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

    pub fn new_metadata_condition(
        ctx: &mut SpendContext,
        new_metadata: M,
    ) -> Result<Condition, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let new_metadata_condition = NewMetadataCondition::<i32, NewMetadataOutput<M, ()>> {
            metadata_updater_reveal: 11,
            // metadata updater will just return solution, so we can set the solution to NewMetadataOutput :)
            metadata_updater_solution: NewMetadataOutput {
                metadata_part: NewMetadataInfo::<M> {
                    new_metadata,
                    new_metadata_updater_puzhash: DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                },
                conditions: (),
            },
        }
        .to_clvm(&mut ctx.allocator)?;

        Ok(Condition::Other(new_metadata_condition))
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub mod tests {
    use core::panic;

    use chia_bls::{PublicKey, SecretKey};
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_secret_keys, test_transaction, Simulator};
    use chia_sdk_types::{Conditions, MeltSingleton, NewMerkleRootCondition};
    use clvmr::sha2::Sha256;
    use rstest::rstest;

    use crate::{DelegationLayer, Launcher, OracleLayer, StandardLayer, WriterLayer};

    use super::*;

    #[derive(Debug, PartialEq, Copy, Clone)]
    pub enum Label {
        None,
        Some,
        New,
    }

    impl Label {
        pub fn value(&self) -> Option<String> {
            match self {
                Label::None => None,
                Label::Some => Some(String::from("label")),
                Label::New => Some(String::from("new_label")),
            }
        }
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    pub enum Description {
        None,
        Some,
        New,
    }

    impl Description {
        pub fn value(&self) -> Option<String> {
            match self {
                Description::None => None,
                Description::Some => Some(String::from("description")),
                Description::New => Some(String::from("new_description")),
            }
        }
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    pub enum RootHash {
        Zero,
        Some,
    }

    impl RootHash {
        pub fn value(&self) -> Bytes32 {
            match self {
                RootHash::Zero => Bytes32::from([0; 32]),
                RootHash::Some => Bytes32::from([1; 32]),
            }
        }
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    pub enum ByteSize {
        None,
        Some,
        New,
    }

    impl ByteSize {
        pub fn value(&self) -> Option<u64> {
            match self {
                ByteSize::None => None,
                ByteSize::Some => Some(1337),
                ByteSize::New => Some(42),
            }
        }
    }

    pub fn metadata_from_tuple(t: (RootHash, Label, Description, ByteSize)) -> DataStoreMetadata {
        DataStoreMetadata {
            root_hash: t.0.value(),
            label: t.1.value(),
            description: t.2.value(),
            bytes: t.3.value(),
        }
    }

    #[tokio::test]
    async fn test_simple_datastore() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [sk]: [SecretKey; 1] = test_secret_keys(1)?.try_into().unwrap();
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let (launch_singleton, datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            DataStoreMetadata::root_hash_only(RootHash::Zero.value()),
            puzzle_hash.into(),
            vec![],
        )?;

        ctx.spend_p2_coin(coin, pk, launch_singleton)?;

        let spends = ctx.take();
        for spend in spends {
            if spend.coin.coin_id() == datastore.info.launcher_id {
                let new_datastore =
                    DataStore::from_spend(&mut ctx.allocator, &spend, &[])?.unwrap();

                assert_eq!(datastore, new_datastore);
            }

            ctx.insert(spend);
        }

        let datastore_inner_spend = StandardLayer::new(pk)
            .spend(ctx, Conditions::new().create_coin(puzzle_hash, 1, vec![]))?;

        let old_datastore_coin = datastore.coin;
        let new_spend = datastore.spend(ctx, datastore_inner_spend)?;

        ctx.insert(new_spend);

        test_transaction(&peer, ctx.take(), &[sk], &sim.config().constants).await;

        // Make sure the datastore was created.
        let coin_state = sim
            .coin_state(old_datastore_coin.coin_id())
            .await
            .expect("expected datastore coin");
        assert_eq!(coin_state.coin, old_datastore_coin);
        assert!(coin_state.spent_height.is_some());

        Ok(())
    }

    #[allow(clippy::similar_names)]
    #[tokio::test]
    async fn test_datastore_with_delegation_layer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, writer_sk]: [SecretKey; 3] =
            test_secret_keys(3)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let writer_pk = writer_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [1; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let admin_puzzle: NodePtr = CurriedProgram {
            program: ctx.standard_puzzle()?,
            args: StandardArgs::new(admin_pk),
        }
        .to_clvm(&mut ctx.allocator)?;
        let admin_puzzle_hash = tree_hash(&ctx.allocator, admin_puzzle);

        let writer_inner_puzzle: NodePtr = CurriedProgram {
            program: ctx.standard_puzzle()?,
            args: StandardArgs::new(writer_pk),
        }
        .to_clvm(&mut ctx.allocator)?;
        let writer_inner_puzzle_hash = tree_hash(&ctx.allocator, writer_inner_puzzle);

        let admin_delegated_puzzle = DelegatedPuzzle::Admin(admin_puzzle_hash);
        let writer_delegated_puzzle = DelegatedPuzzle::Writer(writer_inner_puzzle_hash);

        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let (launch_singleton, datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            DataStoreMetadata::default(),
            owner_puzzle_hash.into(),
            vec![
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        let spends = ctx.take();
        for spend in spends {
            if spend.coin.coin_id() == datastore.info.launcher_id {
                let new_datastore =
                    DataStore::from_spend(&mut ctx.allocator, &spend, &[])?.unwrap();

                assert_eq!(datastore, new_datastore);
            }

            ctx.insert(spend);
        }

        assert_eq!(datastore.info.metadata.root_hash, RootHash::Zero.value());

        // writer: update metadata
        let new_metadata = metadata_from_tuple((
            RootHash::Some,
            Label::Some,
            Description::Some,
            ByteSize::Some,
        ));

        let new_metadata_condition = DataStore::new_metadata_condition(ctx, new_metadata.clone())?;

        let inner_spend = WriterLayer::new(StandardLayer::new(writer_pk))
            .spend(ctx, Conditions::new().with(new_metadata_condition))?;
        let new_spend = datastore.clone().spend(ctx, inner_spend)?;

        let datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend);

        assert_eq!(datastore.info.metadata, new_metadata);

        // admin: remove writer from delegated puzzles
        let delegated_puzzles = vec![admin_delegated_puzzle, oracle_delegated_puzzle];
        let new_merkle_tree = get_merkle_tree(ctx, delegated_puzzles.clone())?;
        let new_merkle_root = new_merkle_tree.root;

        let new_merkle_root_condition = NewMerkleRootCondition {
            new_merkle_root,
            memos: DataStore::<DataStoreMetadata>::get_recreation_memos(
                datastore.info.launcher_id,
                owner_puzzle_hash.into(),
                delegated_puzzles.clone(),
            ),
        }
        .to_clvm(&mut ctx.allocator)?;

        let inner_spend = StandardLayer::new(admin_pk).spend(
            ctx,
            Conditions::new().with(Condition::Other(new_merkle_root_condition)),
        )?;
        let new_spend = datastore.clone().spend(ctx, inner_spend)?;

        let datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend);

        assert!(!datastore.info.delegated_puzzles.is_empty());
        assert_eq!(datastore.info.delegated_puzzles, delegated_puzzles);

        // oracle: just spend :)

        let oracle_layer = OracleLayer::new(oracle_puzzle_hash, oracle_fee).unwrap();
        let inner_datastore_spend = oracle_layer.construct_spend(ctx, ())?;

        let new_spend = datastore.clone().spend(ctx, inner_datastore_spend)?;

        let new_datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend);

        assert_eq!(new_datastore.info, new_datastore.info);
        let datastore = new_datastore;

        // mint a coin that asserts the announcement and has enough value
        let new_coin = sim.mint_coin(owner_puzzle_hash, oracle_fee).await;

        let mut hasher = Sha256::new();
        hasher.update(datastore.coin.puzzle_hash);
        hasher.update(Bytes::new("$".into()).to_vec());

        ctx.spend_p2_coin(
            new_coin,
            owner_pk,
            Conditions::new().assert_puzzle_announcement(Bytes32::new(hasher.finalize())),
        )?;

        // finally, remove delegation layer altogether
        let owner_layer = StandardLayer::new(owner_pk);
        let output_condition = DataStore::<DataStoreMetadata>::owner_create_coin_condition(
            ctx,
            datastore.info.launcher_id,
            owner_puzzle_hash,
            vec![],
            true,
        )?;
        let datastore_remove_delegation_layer_inner_spend =
            owner_layer.spend(ctx, Conditions::new().with(output_condition))?;
        let new_spend = datastore
            .clone()
            .spend(ctx, datastore_remove_delegation_layer_inner_spend)?;

        let new_datastore =
            DataStore::<DataStoreMetadata>::from_spend(&mut ctx.allocator, &new_spend, &[])?
                .unwrap();
        ctx.insert(new_spend);

        assert!(new_datastore.info.delegated_puzzles.is_empty());
        assert_eq!(new_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, admin_sk, writer_sk],
            &sim.config().constants,
        )
        .await;

        // Make sure the datastore was created.
        let coin_state = sim
            .coin_state(new_datastore.coin.parent_coin_info)
            .await
            .expect("expected datastore coin");
        assert_eq!(coin_state.coin, datastore.coin);
        assert!(coin_state.spent_height.is_some());

        Ok(())
    }

    #[derive(PartialEq, Debug, Clone, Copy)]
    pub enum DstAdminLayer {
        None,
        Same,
        New,
    }

    fn assert_delegated_puzzles_contain(
        dps: &[DelegatedPuzzle],
        values: &[DelegatedPuzzle],
        contained: &[bool],
    ) {
        for (i, value) in values.iter().enumerate() {
            assert_eq!(dps.iter().any(|dp| dp == value), contained[i]);
        }
    }

    #[rstest(
    src_with_writer => [true, false],
    src_with_oracle => [true, false],
    dst_with_writer => [true, false],
    dst_with_oracle => [true, false],
    src_meta => [
      (RootHash::Zero, Label::None, Description::None, ByteSize::None),
      (RootHash::Some, Label::Some, Description::Some, ByteSize::Some),
    ],
    dst_meta => [
      (RootHash::Zero, Label::None, Description::None, ByteSize::None),
      (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
      (RootHash::Zero, Label::New, Description::New, ByteSize::New),
    ],
    dst_admin => [
      DstAdminLayer::None,
      DstAdminLayer::Same,
      DstAdminLayer::New,
    ]
  )]
    #[tokio::test]
    async fn test_datastore_admin_transition(
        src_meta: (RootHash, Label, Description, ByteSize),
        src_with_writer: bool,
        // src must have admin layer in this scenario
        src_with_oracle: bool,
        dst_with_writer: bool,
        dst_with_oracle: bool,
        dst_admin: DstAdminLayer,
        dst_meta: (RootHash, Label, Description, ByteSize),
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, admin2_sk, writer_sk]: [SecretKey; 4] =
            test_secret_keys(4)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let admin2_pk = admin2_sk.public_key();
        let writer_pk = writer_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin_pk));
        let admin2_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin2_pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer_pk));
        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let mut src_delegated_puzzles: Vec<DelegatedPuzzle> = vec![];
        src_delegated_puzzles.push(admin_delegated_puzzle);
        if src_with_writer {
            src_delegated_puzzles.push(writer_delegated_puzzle);
        }
        if src_with_oracle {
            src_delegated_puzzles.push(oracle_delegated_puzzle);
        }

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata_from_tuple(src_meta),
            owner_puzzle_hash.into(),
            src_delegated_puzzles.clone(),
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // transition from src to dst
        let mut admin_inner_output = Conditions::new();

        let mut dst_delegated_puzzles: Vec<DelegatedPuzzle> = src_delegated_puzzles.clone();
        if src_with_writer != dst_with_writer
            || src_with_oracle != dst_with_oracle
            || dst_admin != DstAdminLayer::Same
        {
            dst_delegated_puzzles.clear();

            if dst_with_writer {
                dst_delegated_puzzles.push(writer_delegated_puzzle);
            }
            if dst_with_oracle {
                dst_delegated_puzzles.push(oracle_delegated_puzzle);
            }

            match dst_admin {
                DstAdminLayer::None => {}
                DstAdminLayer::Same => {
                    dst_delegated_puzzles.push(admin_delegated_puzzle);
                }
                DstAdminLayer::New => {
                    dst_delegated_puzzles.push(admin2_delegated_puzzle);
                }
            }

            let new_merkle_tree = get_merkle_tree(ctx, dst_delegated_puzzles.clone())?;

            let new_merkle_root_condition = NewMerkleRootCondition {
                new_merkle_root: new_merkle_tree.root,
                memos: DataStore::<DataStoreMetadata>::get_recreation_memos(
                    src_datastore.info.launcher_id,
                    owner_puzzle_hash.into(),
                    dst_delegated_puzzles.clone(),
                ),
            }
            .to_clvm(&mut ctx.allocator)?;

            admin_inner_output =
                admin_inner_output.with(Condition::Other(new_merkle_root_condition));
        }

        if src_meta != dst_meta {
            let new_metadata = metadata_from_tuple(dst_meta);

            admin_inner_output =
                admin_inner_output.with(DataStore::new_metadata_condition(ctx, new_metadata)?);
        }

        // delegated puzzle info + inner puzzle reveal + solution
        let inner_datastore_spend = StandardLayer::new(admin_pk).spend(ctx, admin_inner_output)?;
        let src_datastore_coin = src_datastore.coin;
        let new_spend = src_datastore.clone().spend(ctx, inner_datastore_spend)?;

        let dst_datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &src_datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend);

        assert_eq!(src_datastore.info.delegated_puzzles, src_delegated_puzzles);
        assert_eq!(src_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(src_datastore.info.metadata, metadata_from_tuple(src_meta));

        assert_delegated_puzzles_contain(
            &src_datastore.info.delegated_puzzles,
            &[
                admin2_delegated_puzzle,
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[false, true, src_with_writer, src_with_oracle],
        );

        assert_eq!(dst_datastore.info.delegated_puzzles, dst_delegated_puzzles);
        assert_eq!(dst_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(dst_datastore.info.metadata, metadata_from_tuple(dst_meta));

        assert_delegated_puzzles_contain(
            &dst_datastore.info.delegated_puzzles,
            &[
                admin2_delegated_puzzle,
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[
                dst_admin == DstAdminLayer::New,
                dst_admin == DstAdminLayer::Same,
                dst_with_writer,
                dst_with_oracle,
            ],
        );

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, admin_sk, writer_sk],
            &sim.config().constants,
        )
        .await;

        let src_coin_state = sim
            .coin_state(src_datastore_coin.coin_id())
            .await
            .expect("expected src datastore coin");
        assert_eq!(src_coin_state.coin, src_datastore_coin);
        assert!(src_coin_state.spent_height.is_some());
        let dst_coin_state = sim
            .coin_state(dst_datastore.coin.coin_id())
            .await
            .expect("expected dst datastore coin");
        assert_eq!(dst_coin_state.coin, dst_datastore.coin);
        assert!(dst_coin_state.created_height.is_some());

        Ok(())
    }

    #[rstest(
        src_with_admin => [true, false],
        src_with_writer => [true, false],
        src_with_oracle => [true, false],
        dst_with_admin => [true, false],
        dst_with_writer => [true, false],
        dst_with_oracle => [true, false],
        src_meta => [
          (RootHash::Zero, Label::None, Description::None, ByteSize::None),
          (RootHash::Some, Label::Some, Description::Some, ByteSize::Some),
        ],
        dst_meta => [
          (RootHash::Zero, Label::None, Description::None, ByteSize::None),
          (RootHash::Some, Label::Some, Description::Some, ByteSize::Some),
          (RootHash::Some, Label::New, Description::New, ByteSize::New),
        ],
        change_owner => [true, false],
      )]
    #[tokio::test]
    async fn test_datastore_owner_transition(
        src_meta: (RootHash, Label, Description, ByteSize),
        src_with_admin: bool,
        src_with_writer: bool,
        src_with_oracle: bool,
        dst_with_admin: bool,
        dst_with_writer: bool,
        dst_with_oracle: bool,
        dst_meta: (RootHash, Label, Description, ByteSize),
        change_owner: bool,
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, owner2_sk, admin_sk, writer_sk]: [SecretKey; 4] =
            test_secret_keys(4)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let owner2_pk = owner2_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let writer_pk = writer_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let owner2_puzzle_hash = StandardArgs::curry_tree_hash(owner2_pk).into();
        assert_ne!(owner_puzzle_hash, owner2_puzzle_hash);

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin_pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer_pk));
        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let mut src_delegated_puzzles: Vec<DelegatedPuzzle> = vec![];
        if src_with_admin {
            src_delegated_puzzles.push(admin_delegated_puzzle);
        }
        if src_with_writer {
            src_delegated_puzzles.push(writer_delegated_puzzle);
        }
        if src_with_oracle {
            src_delegated_puzzles.push(oracle_delegated_puzzle);
        }

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata_from_tuple(src_meta),
            owner_puzzle_hash.into(),
            src_delegated_puzzles.clone(),
        )?;
        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // transition from src to dst using owner puzzle
        let mut owner_output_conds = Conditions::new();

        let mut dst_delegated_puzzles: Vec<DelegatedPuzzle> = src_delegated_puzzles.clone();
        let mut hint_new_delegated_puzzles = change_owner;
        if src_with_admin != dst_with_admin
            || src_with_writer != dst_with_writer
            || src_with_oracle != dst_with_oracle
            || dst_delegated_puzzles.is_empty()
        {
            dst_delegated_puzzles.clear();
            hint_new_delegated_puzzles = true;

            if dst_with_admin {
                dst_delegated_puzzles.push(admin_delegated_puzzle);
            }
            if dst_with_writer {
                dst_delegated_puzzles.push(writer_delegated_puzzle);
            }
            if dst_with_oracle {
                dst_delegated_puzzles.push(oracle_delegated_puzzle);
            }
        }

        owner_output_conds =
            owner_output_conds.with(DataStore::<DataStoreMetadata>::owner_create_coin_condition(
                ctx,
                src_datastore.info.launcher_id,
                if change_owner {
                    owner2_puzzle_hash
                } else {
                    owner_puzzle_hash
                },
                dst_delegated_puzzles.clone(),
                hint_new_delegated_puzzles,
            )?);

        if src_meta != dst_meta {
            let new_metadata = metadata_from_tuple(dst_meta);

            owner_output_conds =
                owner_output_conds.with(DataStore::new_metadata_condition(ctx, new_metadata)?);
        }

        // delegated puzzle info + inner puzzle reveal + solution
        let inner_datastore_spend = StandardLayer::new(owner_pk).spend(ctx, owner_output_conds)?;
        let new_spend = src_datastore.clone().spend(ctx, inner_datastore_spend)?;

        let dst_datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &src_datastore.info.delegated_puzzles,
        )?
        .unwrap();

        ctx.insert(new_spend);

        assert_eq!(src_datastore.info.delegated_puzzles, src_delegated_puzzles);
        assert_eq!(src_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(src_datastore.info.metadata, metadata_from_tuple(src_meta));

        assert_delegated_puzzles_contain(
            &src_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[src_with_admin, src_with_writer, src_with_oracle],
        );

        assert_eq!(dst_datastore.info.delegated_puzzles, dst_delegated_puzzles);
        assert_eq!(
            dst_datastore.info.owner_puzzle_hash,
            if change_owner {
                owner2_puzzle_hash
            } else {
                owner_puzzle_hash
            }
        );

        assert_eq!(dst_datastore.info.metadata, metadata_from_tuple(dst_meta));

        assert_delegated_puzzles_contain(
            &dst_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[dst_with_admin, dst_with_writer, dst_with_oracle],
        );

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, admin_sk, writer_sk],
            &sim.config().constants,
        )
        .await;

        let src_coin_state = sim
            .coin_state(src_datastore.coin.coin_id())
            .await
            .expect("expected src datastore coin");
        assert_eq!(src_coin_state.coin, src_datastore.coin);
        assert!(src_coin_state.spent_height.is_some());

        let dst_coin_state = sim
            .coin_state(dst_datastore.coin.coin_id())
            .await
            .expect("expected dst datastore coin");
        assert_eq!(dst_coin_state.coin, dst_datastore.coin);
        assert!(dst_coin_state.created_height.is_some());

        Ok(())
    }

    #[rstest(
    with_admin_layer => [true, false],
    with_oracle_layer => [true, false],
    meta_transition => [
      (
        (RootHash::Zero, Label::None, Description::None, ByteSize::None),
        (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
      ),
      (
        (RootHash::Zero, Label::None, Description::None, ByteSize::None),
        (RootHash::Some, Label::None, Description::None, ByteSize::None),
      ),
      (
        (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
        (RootHash::Some, Label::Some, Description::Some, ByteSize::Some),
      ),
      (
        (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
        (RootHash::Zero, Label::New, Description::New, ByteSize::New),
      ),
      (
        (RootHash::Zero, Label::None, Description::None, ByteSize::None),
        (RootHash::Zero, Label::None, Description::None, ByteSize::Some),
      ),
      (
        (RootHash::Zero, Label::None, Description::None, ByteSize::None),
        (RootHash::Zero, Label::None, Description::Some, ByteSize::Some),
      ),
    ],
  )]
    #[tokio::test]
    async fn test_datastore_writer_transition(
        with_admin_layer: bool,
        with_oracle_layer: bool,
        meta_transition: (
            (RootHash, Label, Description, ByteSize),
            (RootHash, Label, Description, ByteSize),
        ),
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, writer_sk]: [SecretKey; 3] =
            test_secret_keys(3)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let writer_pk = writer_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin_pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer_pk));
        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let mut delegated_puzzles: Vec<DelegatedPuzzle> = vec![];
        delegated_puzzles.push(writer_delegated_puzzle);
        if with_admin_layer {
            delegated_puzzles.push(admin_delegated_puzzle);
        }
        if with_oracle_layer {
            delegated_puzzles.push(oracle_delegated_puzzle);
        }

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata_from_tuple(meta_transition.0),
            owner_puzzle_hash.into(),
            delegated_puzzles.clone(),
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // transition from src to dst using writer (update metadata)
        let new_metadata = metadata_from_tuple(meta_transition.1);
        let new_metadata_condition = DataStore::new_metadata_condition(ctx, new_metadata)?;

        let inner_spend = WriterLayer::new(StandardLayer::new(writer_pk))
            .spend(ctx, Conditions::new().with(new_metadata_condition))?;

        let new_spend = src_datastore.clone().spend(ctx, inner_spend)?;

        let dst_datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &src_datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend.clone());

        assert_eq!(src_datastore.info.delegated_puzzles, delegated_puzzles);
        assert_eq!(src_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(
            src_datastore.info.metadata,
            metadata_from_tuple(meta_transition.0)
        );

        assert_delegated_puzzles_contain(
            &src_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[with_admin_layer, true, with_oracle_layer],
        );

        assert_eq!(dst_datastore.info.delegated_puzzles, delegated_puzzles);
        assert_eq!(dst_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(
            dst_datastore.info.metadata,
            metadata_from_tuple(meta_transition.1)
        );

        assert_delegated_puzzles_contain(
            &dst_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[with_admin_layer, true, with_oracle_layer],
        );

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, admin_sk, writer_sk],
            &sim.config().constants,
        )
        .await;

        let src_coin_state = sim
            .coin_state(src_datastore.coin.coin_id())
            .await
            .expect("expected src datastore coin");
        assert_eq!(src_coin_state.coin, src_datastore.coin);
        assert!(src_coin_state.spent_height.is_some());
        let dst_coin_state = sim
            .coin_state(dst_datastore.coin.coin_id())
            .await
            .expect("expected dst datastore coin");
        assert_eq!(dst_coin_state.coin, dst_datastore.coin);
        assert!(dst_coin_state.created_height.is_some());

        Ok(())
    }

    #[rstest(
    with_admin_layer => [true, false],
    with_writer_layer => [true, false],
    meta => [
      (RootHash::Zero, Label::None, Description::None, ByteSize::None),
      (RootHash::Zero, Label::None, Description::None, ByteSize::Some),
      (RootHash::Zero, Label::None, Description::Some, ByteSize::Some),
      (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
    ],
  )]
    #[tokio::test]
    async fn test_datastore_oracle_transition(
        with_admin_layer: bool,
        with_writer_layer: bool,
        meta: (RootHash, Label, Description, ByteSize),
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, writer_sk, dude_sk]: [SecretKey; 4] =
            test_secret_keys(4)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let writer_pk = writer_sk.public_key();
        let dude_pk = dude_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let dude_puzzle_hash = StandardArgs::curry_tree_hash(dude_pk).into();

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin_pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer_pk));
        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let mut delegated_puzzles: Vec<DelegatedPuzzle> = vec![];
        delegated_puzzles.push(oracle_delegated_puzzle);

        if with_admin_layer {
            delegated_puzzles.push(admin_delegated_puzzle);
        }
        if with_writer_layer {
            delegated_puzzles.push(writer_delegated_puzzle);
        }

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata_from_tuple(meta),
            owner_puzzle_hash.into(),
            delegated_puzzles.clone(),
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // 'dude' spends oracle
        let inner_datastore_spend = OracleLayer::new(oracle_puzzle_hash, oracle_fee)
            .unwrap()
            .spend(ctx)?;
        let new_spend = src_datastore.clone().spend(ctx, inner_datastore_spend)?;

        let dst_datastore = DataStore::from_spend(
            &mut ctx.allocator,
            &new_spend,
            &src_datastore.info.delegated_puzzles,
        )?
        .unwrap();
        ctx.insert(new_spend);

        assert_eq!(src_datastore.info, dst_datastore.info);

        // mint a coin that asserts the announcement and has enough value
        let mut hasher = Sha256::new();
        hasher.update(src_datastore.coin.puzzle_hash);
        hasher.update(Bytes::new("$".into()).to_vec());

        let new_coin = sim.mint_coin(dude_puzzle_hash, oracle_fee).await;
        ctx.spend_p2_coin(
            new_coin,
            dude_pk,
            Conditions::new().assert_puzzle_announcement(Bytes32::new(hasher.finalize())),
        )?;

        // asserts

        assert_eq!(src_datastore.info.delegated_puzzles, delegated_puzzles);
        assert_eq!(src_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(src_datastore.info.metadata, metadata_from_tuple(meta));

        assert_delegated_puzzles_contain(
            &src_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[with_admin_layer, with_writer_layer, true],
        );

        assert_eq!(dst_datastore.info.delegated_puzzles, delegated_puzzles);
        assert_eq!(dst_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(dst_datastore.info.metadata, metadata_from_tuple(meta));

        assert_delegated_puzzles_contain(
            &dst_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[with_admin_layer, with_writer_layer, true],
        );

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, dude_sk],
            &sim.config().constants,
        )
        .await;

        let src_datastore_coin_id = src_datastore.coin.coin_id();
        let src_coin_state = sim
            .coin_state(src_datastore_coin_id)
            .await
            .expect("expected src datastore coin");
        assert_eq!(src_coin_state.coin, src_datastore.coin);
        assert!(src_coin_state.spent_height.is_some());
        let dst_coin_state = sim
            .coin_state(dst_datastore.coin.coin_id())
            .await
            .expect("expected dst datastore coin");
        assert_eq!(dst_coin_state.coin, dst_datastore.coin);
        assert!(dst_coin_state.created_height.is_some());

        let oracle_coin = Coin::new(src_datastore_coin_id, oracle_puzzle_hash, oracle_fee);
        let oracle_coin_state = sim
            .coin_state(oracle_coin.coin_id())
            .await
            .expect("expected oracle coin");
        assert_eq!(oracle_coin_state.coin, oracle_coin);
        assert!(oracle_coin_state.created_height.is_some());

        Ok(())
    }

    #[rstest(
    with_admin_layer => [true, false],
    with_writer_layer => [true, false],
    with_oracle_layer => [true, false],
    meta => [
      (RootHash::Zero, Label::None, Description::None, ByteSize::None),
      (RootHash::Zero, Label::Some, Description::Some, ByteSize::Some),
    ],
  )]
    #[tokio::test]
    async fn test_melt(
        with_admin_layer: bool,
        with_writer_layer: bool,
        with_oracle_layer: bool,
        meta: (RootHash, Label, Description, ByteSize),
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, writer_sk]: [SecretKey; 3] =
            test_secret_keys(3)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let admin_pk = admin_sk.public_key();
        let writer_pk = writer_sk.public_key();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin_pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer_pk));
        let oracle_delegated_puzzle = DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee);

        let mut delegated_puzzles: Vec<DelegatedPuzzle> = vec![];
        if with_admin_layer {
            delegated_puzzles.push(admin_delegated_puzzle);
        }
        if with_writer_layer {
            delegated_puzzles.push(writer_delegated_puzzle);
        }
        if with_oracle_layer {
            delegated_puzzles.push(oracle_delegated_puzzle);
        }

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata_from_tuple(meta),
            owner_puzzle_hash.into(),
            delegated_puzzles.clone(),
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // owner melts
        let output_conds = Conditions::new().with(Condition::Other(
            MeltSingleton {}.to_clvm(&mut ctx.allocator)?,
        ));
        let inner_datastore_spend = StandardLayer::new(owner_pk).spend(ctx, output_conds)?;

        let new_spend = src_datastore.clone().spend(ctx, inner_datastore_spend)?;
        ctx.insert(new_spend);

        // asserts

        assert_eq!(src_datastore.info.owner_puzzle_hash, owner_puzzle_hash);

        assert_eq!(src_datastore.info.metadata, metadata_from_tuple(meta));

        assert_delegated_puzzles_contain(
            &src_datastore.info.delegated_puzzles,
            &[
                admin_delegated_puzzle,
                writer_delegated_puzzle,
                oracle_delegated_puzzle,
            ],
            &[with_admin_layer, with_writer_layer, with_oracle_layer],
        );

        test_transaction(&peer, ctx.take(), &[owner_sk], &sim.config().constants).await;

        let src_coin_state = sim
            .coin_state(src_datastore.coin.coin_id())
            .await
            .expect("expected src datastore coin");
        assert_eq!(src_coin_state.coin, src_datastore.coin);
        assert!(src_coin_state.spent_height.is_some()); // tx happened

        Ok(())
    }

    enum AttackerPuzzle {
        Admin,
        Writer,
    }

    impl AttackerPuzzle {
        fn get_spend(
            &self,
            ctx: &mut SpendContext,
            attacker_pk: PublicKey,
            output_conds: Conditions,
        ) -> Result<Spend, DriverError> {
            Ok(match self {
                AttackerPuzzle::Admin => {
                    StandardLayer::new(attacker_pk).spend(ctx, output_conds)?
                }

                AttackerPuzzle::Writer => {
                    WriterLayer::new(StandardLayer::new(attacker_pk)).spend(ctx, output_conds)?
                }
            })
        }
    }

    #[rstest(
    puzzle => [AttackerPuzzle::Admin, AttackerPuzzle::Writer],
  )]
    #[tokio::test]
    async fn test_create_coin_filer(puzzle: AttackerPuzzle) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;

        let [owner_sk, attacker_sk]: [SecretKey; 2] = test_secret_keys(2)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let attacker_pk = attacker_sk.public_key();

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let attacker_puzzle_hash = StandardArgs::curry_tree_hash(attacker_pk);
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let ctx = &mut SpendContext::new();

        let delegated_puzzle = match puzzle {
            AttackerPuzzle::Admin => DelegatedPuzzle::Admin(attacker_puzzle_hash),
            AttackerPuzzle::Writer => DelegatedPuzzle::Writer(attacker_puzzle_hash),
        };

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            DataStoreMetadata::default(),
            owner_puzzle_hash.into(),
            vec![delegated_puzzle],
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // delegated puzzle tries to steal the coin
        let inner_datastore_spend = puzzle.get_spend(
            ctx,
            attacker_pk,
            Conditions::new().with(Condition::CreateCoin(CreateCoin {
                puzzle_hash: attacker_puzzle_hash.into(),
                amount: 1,
                memos: vec![],
            })),
        )?;

        let new_spend = src_datastore.spend(ctx, inner_datastore_spend)?;

        let puzzle_reveal_ptr = ctx.alloc(&new_spend.puzzle_reveal)?;
        let solution_ptr = ctx.alloc(&new_spend.solution)?;
        match ctx.run(puzzle_reveal_ptr, solution_ptr) {
            Ok(_) => panic!("expected error"),
            Err(err) => match err {
                DriverError::Eval(eval_err) => {
                    assert_eq!(eval_err.1, "clvm raise");
                }
                _ => panic!("expected 'clvm raise' error"),
            },
        }

        Ok(())
    }

    #[rstest(
    puzzle => [AttackerPuzzle::Admin, AttackerPuzzle::Writer],
  )]
    #[tokio::test]
    async fn test_melt_filter(puzzle: AttackerPuzzle) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;

        let [owner_sk, attacker_sk]: [SecretKey; 2] = test_secret_keys(2)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let attacker_pk = attacker_sk.public_key();

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk).into();
        let coin = sim.mint_coin(owner_puzzle_hash, 1).await;

        let attacker_puzzle_hash = StandardArgs::curry_tree_hash(attacker_pk);

        let ctx = &mut SpendContext::new();

        let delegated_puzzle = match puzzle {
            AttackerPuzzle::Admin => DelegatedPuzzle::Admin(attacker_puzzle_hash),
            AttackerPuzzle::Writer => DelegatedPuzzle::Writer(attacker_puzzle_hash),
        };

        let (launch_singleton, src_datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            DataStoreMetadata::default(),
            owner_puzzle_hash.into(),
            vec![delegated_puzzle],
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        // attacker tries to melt the coin via delegated puzzle
        let conds = Conditions::new().with(Condition::Other(
            MeltSingleton {}.to_clvm(&mut ctx.allocator)?,
        ));
        let inner_datastore_spend = puzzle.get_spend(ctx, attacker_pk, conds)?;

        let new_spend = src_datastore.spend(ctx, inner_datastore_spend)?;

        let puzzle_reveal_ptr = ctx.alloc(&new_spend.puzzle_reveal)?;
        let solution_ptr = ctx.alloc(&new_spend.solution)?;
        match ctx.run(puzzle_reveal_ptr, solution_ptr) {
            Ok(_) => panic!("expected error"),
            Err(err) => match err {
                DriverError::Eval(eval_err) => {
                    assert_eq!(eval_err.1, "clvm raise");
                    Ok(())
                }
                _ => panic!("expected 'clvm raise' error"),
            },
        }
    }

    #[rstest(
        test_puzzle => [AttackerPuzzle::Admin, AttackerPuzzle::Writer],
        new_merkle_root => [RootHash::Zero, RootHash::Some],
        memos => [vec![], vec![RootHash::Zero], vec![RootHash::Some]],
    )]
    fn test_new_merkle_root_filter(
        test_puzzle: AttackerPuzzle,
        new_merkle_root: RootHash,
        memos: Vec<RootHash>,
    ) -> anyhow::Result<()> {
        let [attacker_sk]: [SecretKey; 1] = test_secret_keys(1)?.try_into().unwrap();

        let attacker_pk = attacker_sk.public_key();

        let ctx = &mut SpendContext::new();

        let condition_output = Conditions::new().with(Condition::Other(
            NewMerkleRootCondition::<Bytes32> {
                new_merkle_root: new_merkle_root.value(),
                memos: memos.into_iter().map(|m| m.value()).collect(),
            }
            .to_clvm(&mut ctx.allocator)?,
        ));

        let spend = test_puzzle.get_spend(ctx, attacker_pk, condition_output)?;

        match ctx.run(spend.puzzle, spend.solution) {
            Ok(_) => match test_puzzle {
                AttackerPuzzle::Admin => Ok(()),
                AttackerPuzzle::Writer => panic!("expected error from writer puzzle"),
            },
            Err(err) => match err {
                DriverError::Eval(eval_err) => match test_puzzle {
                    AttackerPuzzle::Admin => panic!("expected admin puzzle to run normally"),
                    AttackerPuzzle::Writer => {
                        assert_eq!(eval_err.1, "clvm raise");
                        Ok(())
                    }
                },
                _ => panic!("other error encountered"),
            },
        }
    }

    #[rstest(
    puzzle => [AttackerPuzzle::Admin, AttackerPuzzle::Writer],
    new_root_hash => [RootHash::Zero, RootHash::Some],
    new_updater_ph => [RootHash::Zero.value().into(), DL_METADATA_UPDATER_PUZZLE_HASH],
    output_conditions => [false, true],
  )]
    fn test_metadata_filter(
        puzzle: AttackerPuzzle,
        new_root_hash: RootHash,
        new_updater_ph: TreeHash,
        output_conditions: bool,
    ) -> anyhow::Result<()> {
        let should_error_out =
            output_conditions || new_updater_ph != DL_METADATA_UPDATER_PUZZLE_HASH;

        let [attacker_sk]: [SecretKey; 1] = test_secret_keys(1)?.try_into().unwrap();

        let attacker_pk = attacker_sk.public_key();

        let ctx = &mut SpendContext::new();

        let new_metadata_condition = Condition::Other(
            NewMetadataCondition {
                metadata_updater_reveal: 11,
                metadata_updater_solution: NewMetadataOutput {
                    metadata_part: NewMetadataInfo {
                        new_metadata: DataStoreMetadata::root_hash_only(new_root_hash.value()),
                        new_metadata_updater_puzhash: new_updater_ph.into(),
                    },
                    conditions: if output_conditions {
                        vec![CreateCoin {
                            puzzle_hash: [0; 32].into(),
                            amount: 1,
                            memos: vec![],
                        }]
                    } else {
                        vec![]
                    },
                },
            }
            .to_clvm(&mut ctx.allocator)?,
        );

        let inner_spend = puzzle.get_spend(
            ctx,
            attacker_pk,
            Conditions::new().with(new_metadata_condition),
        )?;

        let delegated_puzzles = match puzzle {
            AttackerPuzzle::Admin => {
                vec![DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(
                    attacker_pk,
                ))]
            }
            AttackerPuzzle::Writer => vec![DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(
                attacker_pk,
            ))],
        };
        let merkle_tree = get_merkle_tree(ctx, delegated_puzzles.clone())?;

        let delegation_layer =
            DelegationLayer::new(Bytes32::default(), Bytes32::default(), merkle_tree.root);

        let puzzle_ptr = delegation_layer.construct_puzzle(ctx)?;

        let delegated_puzzle_hash = ctx.tree_hash(inner_spend.puzzle);
        let solution_ptr = delegation_layer.construct_solution(
            ctx,
            DelegationLayerSolution {
                merkle_proof: merkle_tree.get_proof(delegated_puzzle_hash.into()),
                puzzle_reveal: inner_spend.puzzle,
                puzzle_solution: inner_spend.solution,
            },
        )?;

        match ctx.run(puzzle_ptr, solution_ptr) {
            Ok(_) => {
                if should_error_out {
                    panic!("expected puzzle to error out");
                } else {
                    Ok(())
                }
            }
            Err(err) => match err {
                DriverError::Eval(eval_err) => {
                    if should_error_out {
                        if output_conditions {
                            assert_eq!(eval_err.1, "= on list");
                        } else {
                            assert_eq!(eval_err.1, "clvm raise");
                        }
                        Ok(())
                    } else {
                        panic!("expected puzzle to not error out");
                    }
                }
                _ => panic!("unexpected error while evaluating puzzle"),
            },
        }
    }

    #[rstest(
    transition => [
      (RootHash::Zero, RootHash::Zero, true),
      (RootHash::Zero, RootHash::Some, false),
      (RootHash::Zero, RootHash::Some, true),
      (RootHash::Some, RootHash::Some, true),
      (RootHash::Some, RootHash::Some, false),
      (RootHash::Some, RootHash::Some, true),
    ]
  )]
    #[tokio::test]
    async fn test_old_memo_format(transition: (RootHash, RootHash, bool)) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, owner2_sk]: [SecretKey; 2] = test_secret_keys(2)?.try_into().unwrap();

        let owner_pk = owner_sk.public_key();
        let owner2_pk = owner2_sk.public_key();

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner_pk);
        let coin = sim.mint_coin(owner_puzzle_hash.into(), 1).await;

        let owner2_puzzle_hash = StandardArgs::curry_tree_hash(owner2_pk);

        let ctx = &mut SpendContext::new();

        // launch using old memos scheme
        let launcher = Launcher::new(coin.coin_id(), 1);
        let inner_puzzle_hash: TreeHash = owner_puzzle_hash;

        let first_root_hash: RootHash = transition.0;
        let metadata_ptr = ctx.alloc(&vec![first_root_hash.value()])?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);
        let state_layer_hash = CurriedProgram {
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs::<TreeHash, TreeHash> {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: metadata_hash,
                metadata_updater_puzzle_hash: DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                inner_puzzle: inner_puzzle_hash,
            },
        }
        .tree_hash();

        // https://github.com/Chia-Network/chia-blockchain/blob/4ffb6dfa6f53f6cd1920bcc775e27377a771fbec/chia/wallet/db_wallet/db_wallet_puzzles.py#L59
        // kv_list = 'memos': (root_hash inner_puzzle_hash)
        let kv_list = vec![first_root_hash.value(), owner_puzzle_hash.into()];

        let launcher_coin = launcher.coin();
        let (launcher_conds, eve_coin) = launcher.spend(ctx, state_layer_hash.into(), kv_list)?;

        ctx.spend_p2_coin(coin, owner_pk, launcher_conds)?;

        let spends = ctx.take();
        spends
            .clone()
            .into_iter()
            .for_each(|spend| ctx.insert(spend));

        let datastore_from_launcher = spends
            .into_iter()
            .find(|spend| spend.coin.coin_id() == eve_coin.parent_coin_info)
            .map(|spend| {
                DataStore::from_spend(&mut ctx.allocator, &spend, &[])
                    .unwrap()
                    .unwrap()
            })
            .expect("expected launcher spend");

        assert_eq!(
            datastore_from_launcher.info.metadata,
            DataStoreMetadata::root_hash_only(first_root_hash.value())
        );
        assert_eq!(
            datastore_from_launcher.info.owner_puzzle_hash,
            owner_puzzle_hash.into()
        );
        assert!(datastore_from_launcher.info.delegated_puzzles.is_empty());

        assert_eq!(
            datastore_from_launcher.info.launcher_id,
            eve_coin.parent_coin_info
        );
        assert_eq!(datastore_from_launcher.coin.coin_id(), eve_coin.coin_id());

        match datastore_from_launcher.proof {
            Proof::Eve(proof) => {
                assert_eq!(
                    proof.parent_parent_coin_info,
                    launcher_coin.parent_coin_info
                );
                assert_eq!(proof.parent_amount, launcher_coin.amount);
            }
            Proof::Lineage(_) => panic!("expected eve (not lineage) proof for info_from_launcher"),
        }

        // now spend the signleton using old memo format and check that info is parsed correctly

        let mut inner_spend_conditions = Conditions::new();

        let second_root_hash: RootHash = transition.1;

        let new_metadata = DataStoreMetadata::root_hash_only(second_root_hash.value());
        if second_root_hash != first_root_hash {
            inner_spend_conditions = inner_spend_conditions.with(
                DataStore::new_metadata_condition(ctx, new_metadata.clone())?,
            );
        }

        let new_owner: bool = transition.2;
        let new_inner_ph: Bytes32 = if new_owner {
            owner2_puzzle_hash.into()
        } else {
            owner_puzzle_hash.into()
        };

        // https://github.com/Chia-Network/chia-blockchain/blob/4ffb6dfa6f53f6cd1920bcc775e27377a771fbec/chia/data_layer/data_layer_wallet.py#L526
        // memos are (launcher_id root_hash inner_puzzle_hash)
        inner_spend_conditions = inner_spend_conditions.with(Condition::CreateCoin(CreateCoin {
            puzzle_hash: new_inner_ph,
            amount: 1,
            memos: vec![
                launcher_coin.coin_id().into(),
                second_root_hash.value().into(),
                new_inner_ph.into(),
            ],
        }));

        let inner_spend = StandardLayer::new(owner_pk).spend(ctx, inner_spend_conditions)?;
        let spend = datastore_from_launcher.clone().spend(ctx, inner_spend)?;

        let new_datastore = DataStore::<DataStoreMetadata>::from_spend(
            &mut ctx.allocator,
            &spend,
            &datastore_from_launcher.info.delegated_puzzles,
        )?
        .unwrap();

        assert_eq!(
            new_datastore.info.metadata,
            DataStoreMetadata::root_hash_only(second_root_hash.value())
        );

        assert!(new_datastore.info.delegated_puzzles.is_empty());

        assert_eq!(new_datastore.info.owner_puzzle_hash, new_inner_ph);
        assert_eq!(new_datastore.info.launcher_id, eve_coin.parent_coin_info);

        assert_eq!(
            new_datastore.coin.parent_coin_info,
            datastore_from_launcher.coin.coin_id()
        );
        assert_eq!(
            new_datastore.coin.puzzle_hash,
            SingletonArgs::curry_tree_hash(
                datastore_from_launcher.info.launcher_id,
                CurriedProgram {
                    program: NFT_STATE_LAYER_PUZZLE_HASH,
                    args: NftStateLayerArgs::<TreeHash, DataStoreMetadata> {
                        mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                        metadata: new_metadata,
                        metadata_updater_puzzle_hash: DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                        inner_puzzle: new_inner_ph.into(),
                    },
                }
                .tree_hash()
            )
            .into()
        );
        assert_eq!(new_datastore.coin.amount, 1);

        match new_datastore.proof {
            Proof::Lineage(proof) => {
                assert_eq!(proof.parent_parent_coin_info, eve_coin.parent_coin_info);
                assert_eq!(proof.parent_amount, eve_coin.amount);
                assert_eq!(
                    proof.parent_inner_puzzle_hash,
                    CurriedProgram {
                        program: NFT_STATE_LAYER_PUZZLE_HASH,
                        args: NftStateLayerArgs::<TreeHash, DataStoreMetadata> {
                            mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                            metadata: datastore_from_launcher.info.metadata,
                            metadata_updater_puzzle_hash: DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                            inner_puzzle: owner_puzzle_hash,
                        },
                    }
                    .tree_hash()
                    .into()
                );
            }
            Proof::Eve(_) => panic!("expected lineage (not eve) proof for new_info"),
        }

        ctx.insert(spend);

        test_transaction(
            &peer,
            ctx.take(),
            &[owner_sk, owner2_sk],
            &sim.config().constants,
        )
        .await;

        let eve_coin_state = sim
            .coin_state(eve_coin.coin_id())
            .await
            .expect("expected eve coin");
        assert!(eve_coin_state.created_height.is_some());

        Ok(())
    }
}
