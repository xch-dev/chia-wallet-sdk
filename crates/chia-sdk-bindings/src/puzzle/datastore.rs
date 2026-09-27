use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::standard::StandardArgs;
use chia_sdk_driver::{
    Datastore as SdkDatastore, DatastoreMetadata, DelegatedPuzzle, Launcher, Layer, OracleLayer,
    SpendContext, SpendWithConditions, StandardLayer, get_merkle_tree,
};
use chia_sdk_types::{
    Condition, Conditions, conditions::UpdateDatastoreMerkleRoot,
    puzzles::DL_METADATA_UPDATER_PUZZLE_HASH,
};
use clvm_traits::{ToClvm, clvm_list};
use clvm_utils::ToTreeHash;

use crate::{Clvm, Program, Proof, Spend};

/// Fields the curated NFT stake/refresh actions need from an on-chain `Datastore`.
/// Matches slot-machine `curated_datastore_fields`.
#[derive(Clone)]
pub struct CuratedDatastoreFields {
    pub root_hash: Bytes32,
    pub metadata_rest_hash: Option<Bytes32>,
    pub metadata_updater_hash_hash: Bytes32,
    pub inner_puzzle_hash: Bytes32,
}

/// Result of `Clvm::mint_datastore` — parent conditions plus the eve `Datastore`.
#[derive(Clone)]
pub struct MintedDatastore {
    pub datastore: Datastore,
    pub parent_conditions: Vec<Program>,
}

/// Synced `Datastore` coin. Parsing and oracle spend mirror slot-machine's
/// `sync_datastore` / `spend_datastore_oracle` helpers.
#[derive(Clone)]
pub struct Datastore {
    clvm: Arc<Mutex<SpendContext>>,
    datastore: Arc<Mutex<SdkDatastore<DatastoreMetadata>>>,
}

fn oracle_delegated_puzzles() -> Vec<DelegatedPuzzle> {
    // Same as slot-machine `oracle_delegated_puzzles` / `delegated_puzzles`.
    vec![DelegatedPuzzle::Oracle(Bytes32::default(), 0)]
}

fn curated_fields(
    ctx: &mut SpendContext,
    datastore: &SdkDatastore<DatastoreMetadata>,
) -> Result<CuratedDatastoreFields> {
    let metadata_rest_hash = datastore.info.metadata.label.as_ref().map(|label| {
        let description = datastore.info.metadata.description.as_deref().unwrap_or("");
        clvm_list!(("l", label.as_str()), ("d", description))
            .tree_hash()
            .into()
    });

    let dl_metadata_updater_hash: Bytes32 = DL_METADATA_UPDATER_PUZZLE_HASH.into();
    Ok(CuratedDatastoreFields {
        root_hash: datastore.info.metadata.root_hash,
        metadata_rest_hash,
        metadata_updater_hash_hash: dl_metadata_updater_hash.tree_hash().into(),
        inner_puzzle_hash: datastore.info.delegation_layer_puzzle_hash(ctx)?.into(),
    })
}

fn wrap_datastore(
    clvm: Arc<Mutex<SpendContext>>,
    datastore: SdkDatastore<DatastoreMetadata>,
) -> Datastore {
    Datastore {
        clvm,
        datastore: Arc::new(Mutex::new(datastore)),
    }
}

pub trait DatastoreMetadataExt {}

impl DatastoreMetadataExt for DatastoreMetadata {}

pub trait DelegatedPuzzleExt: Sized {
    fn oracle(oracle_puzzle_hash: Bytes32, oracle_fee: u64) -> Result<Self>;
    fn admin(puzzle_hash: Bytes32) -> Result<Self>;
    fn writer(inner_puzzle_hash: Bytes32) -> Result<Self>;
    fn admin_from_key(synthetic_key: PublicKey) -> Result<Self>;
    fn writer_from_key(synthetic_key: PublicKey) -> Result<Self>;

    fn to_admin(&self) -> Result<Option<Bytes32>>;
    fn to_writer(&self) -> Result<Option<Bytes32>>;
    fn to_oracle(&self) -> Result<Option<DelegatedPuzzleOracle>>;
}

impl DelegatedPuzzleExt for DelegatedPuzzle {
    fn oracle(oracle_puzzle_hash: Bytes32, oracle_fee: u64) -> Result<Self> {
        Ok(Self::Oracle(oracle_puzzle_hash, oracle_fee))
    }

    fn admin(puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self::Admin(puzzle_hash.into()))
    }

    fn writer(inner_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self::Writer(inner_puzzle_hash.into()))
    }

    fn admin_from_key(synthetic_key: PublicKey) -> Result<Self> {
        Ok(Self::Admin(StandardArgs::curry_tree_hash(synthetic_key)))
    }

    fn writer_from_key(synthetic_key: PublicKey) -> Result<Self> {
        Ok(Self::Writer(StandardArgs::curry_tree_hash(synthetic_key)))
    }

    fn to_admin(&self) -> Result<Option<Bytes32>> {
        Ok(match self {
            Self::Admin(puzzle_hash) => Some((*puzzle_hash).into()),
            _ => None,
        })
    }

    fn to_writer(&self) -> Result<Option<Bytes32>> {
        Ok(match self {
            Self::Writer(inner_puzzle_hash) => Some((*inner_puzzle_hash).into()),
            _ => None,
        })
    }

    fn to_oracle(&self) -> Result<Option<DelegatedPuzzleOracle>> {
        Ok(match *self {
            Self::Oracle(oracle_puzzle_hash, oracle_fee) => Some(DelegatedPuzzleOracle {
                oracle_puzzle_hash,
                oracle_fee,
            }),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct DelegatedPuzzleOracle {
    pub oracle_puzzle_hash: Bytes32,
    pub oracle_fee: u64,
}

impl Datastore {
    pub fn coin(&self) -> Result<Coin> {
        Ok(self.datastore.lock().unwrap().coin)
    }

    pub fn proof(&self) -> Result<Proof> {
        Ok(Proof::from(self.datastore.lock().unwrap().proof))
    }

    pub fn launcher_id(&self) -> Result<Bytes32> {
        Ok(self.datastore.lock().unwrap().info.launcher_id)
    }

    pub fn owner_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.datastore.lock().unwrap().info.owner_puzzle_hash)
    }

    pub fn metadata(&self) -> Result<DatastoreMetadata> {
        Ok(self.datastore.lock().unwrap().info.metadata.clone())
    }

    pub fn root_hash(&self) -> Result<Bytes32> {
        Ok(self.datastore.lock().unwrap().info.metadata.root_hash)
    }

    pub fn label(&self) -> Result<Option<String>> {
        Ok(self.datastore.lock().unwrap().info.metadata.label.clone())
    }

    pub fn description(&self) -> Result<Option<String>> {
        Ok(self
            .datastore
            .lock()
            .unwrap()
            .info
            .metadata
            .description
            .clone())
    }

    pub fn bytes(&self) -> Result<Option<u64>> {
        Ok(self.datastore.lock().unwrap().info.metadata.bytes)
    }

    pub fn size_proof(&self) -> Result<Option<String>> {
        Ok(self
            .datastore
            .lock()
            .unwrap()
            .info
            .metadata
            .size_proof
            .clone())
    }

    pub fn delegated_puzzles(&self) -> Result<Vec<DelegatedPuzzle>> {
        Ok(self
            .datastore
            .lock()
            .unwrap()
            .info
            .delegated_puzzles
            .clone())
    }

    pub fn curated_fields(&self) -> Result<CuratedDatastoreFields> {
        let mut ctx = self.clvm.lock().unwrap();
        let datastore = self.datastore.lock().unwrap();
        curated_fields(&mut ctx, &datastore)
    }

    /// Build the `CREATE_COIN` condition an owner uses to recreate (or re-own) the store.
    pub fn owner_create_coin_condition(
        &self,
        new_owner_puzzle_hash: Bytes32,
        new_delegated_puzzles: Vec<DelegatedPuzzle>,
        hint_delegated_puzzles: bool,
    ) -> Result<Program> {
        let mut ctx = self.clvm.lock().unwrap();
        let launcher_id = self.datastore.lock().unwrap().info.launcher_id;
        let condition = SdkDatastore::<()>::owner_create_coin_condition(
            &mut ctx,
            launcher_id,
            new_owner_puzzle_hash,
            new_delegated_puzzles,
            hint_delegated_puzzles,
        )?;
        Ok(Program(self.clvm.clone(), condition.to_clvm(&mut ctx)?))
    }

    /// Build the NFT metadata-update condition for a new `Datastore` metadata value.
    pub fn new_metadata_condition(&self, new_metadata: DatastoreMetadata) -> Result<Program> {
        let mut ctx = self.clvm.lock().unwrap();
        let condition = SdkDatastore::new_metadata_condition(&mut ctx, new_metadata)?;
        Ok(Program(self.clvm.clone(), condition.to_clvm(&mut ctx)?))
    }

    /// Spend with an arbitrary inner spend and insert the coin spend into the CLVM context.
    /// Returns the child `Datastore` parsed with this store's current delegated puzzles.
    pub fn spend(&self, inner_spend: Spend) -> Result<Datastore> {
        let mut ctx = self.clvm.lock().unwrap();
        let datastore = self.datastore.lock().unwrap().clone();
        let parent_delegated_puzzles = datastore.info.delegated_puzzles.clone();
        let dl_spend = datastore.spend(
            &mut ctx,
            chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
        )?;
        let new_datastore = SdkDatastore::<DatastoreMetadata>::from_spend(
            &mut ctx,
            &dl_spend,
            &parent_delegated_puzzles,
        )?
        .ok_or_else(|| Error::Custom("Failed to parse Datastore child from spend.".to_string()))?;
        ctx.insert(dl_spend);
        Ok(wrap_datastore(self.clvm.clone(), new_datastore))
    }

    /// Spend with `Oracle(zero, 0)` and insert the coin spend into the CLVM context.
    pub fn spend_oracle(&self) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();
        let datastore = self.datastore.lock().unwrap().clone();
        let oracle = OracleLayer::new(Bytes32::default(), 0)
            .ok_or_else(|| Error::Custom("Invalid oracle fee for Datastore spend.".to_string()))?;
        let inner_spend = oracle.construct_spend(&mut ctx, ())?;
        let dl_spend = datastore.spend(&mut ctx, inner_spend)?;
        ctx.insert(dl_spend);
        Ok(())
    }

    /// Owner metadata update (slot-machine `datastore update` path).
    /// Recreates with the same owner/delegated puzzles and applies new metadata.
    pub fn update_metadata_as_owner(
        &self,
        owner_synthetic_key: PublicKey,
        new_metadata: DatastoreMetadata,
    ) -> Result<Datastore> {
        let mut ctx = self.clvm.lock().unwrap();
        let datastore = self.datastore.lock().unwrap().clone();
        let parent_delegated_puzzles = datastore.info.delegated_puzzles.clone();

        let recreate = SdkDatastore::<()>::owner_create_coin_condition(
            &mut ctx,
            datastore.info.launcher_id,
            datastore.info.owner_puzzle_hash,
            parent_delegated_puzzles.clone(),
            false,
        )?;
        let metadata_condition = SdkDatastore::new_metadata_condition(&mut ctx, new_metadata)?;

        let inner_spend = StandardLayer::new(owner_synthetic_key).spend_with_conditions(
            &mut ctx,
            Conditions::new().with(recreate).with(metadata_condition),
        )?;
        let dl_spend = datastore.spend(&mut ctx, inner_spend)?;
        let new_datastore = SdkDatastore::<DatastoreMetadata>::from_spend(
            &mut ctx,
            &dl_spend,
            &parent_delegated_puzzles,
        )?
        .ok_or_else(|| {
            Error::Custom("Failed to parse Datastore after metadata update.".to_string())
        })?;
        ctx.insert(dl_spend);
        Ok(wrap_datastore(self.clvm.clone(), new_datastore))
    }

    /// Change owner puzzle hash and/or delegated puzzles.
    /// Provide exactly one of `owner_synthetic_key` or `admin_synthetic_key`.
    pub fn update_ownership(
        &self,
        new_owner_puzzle_hash: Bytes32,
        new_delegated_puzzles: Vec<DelegatedPuzzle>,
        owner_synthetic_key: Option<PublicKey>,
        admin_synthetic_key: Option<PublicKey>,
    ) -> Result<Datastore> {
        let mut ctx = self.clvm.lock().unwrap();
        let datastore = self.datastore.lock().unwrap().clone();
        let parent_delegated_puzzles = datastore.info.delegated_puzzles.clone();

        let (inner_key, update_condition) = match (owner_synthetic_key, admin_synthetic_key) {
            (Some(owner_key), None) => {
                let condition = SdkDatastore::<()>::owner_create_coin_condition(
                    &mut ctx,
                    datastore.info.launcher_id,
                    new_owner_puzzle_hash,
                    new_delegated_puzzles,
                    true,
                )?;
                (owner_key, condition)
            }
            (None, Some(admin_key)) => {
                let merkle_tree = get_merkle_tree(&mut ctx, new_delegated_puzzles.clone())?;
                let memos = SdkDatastore::<DatastoreMetadata>::get_recreation_memos(
                    datastore.info.launcher_id,
                    new_owner_puzzle_hash.into(),
                    new_delegated_puzzles,
                );
                let ptr = ctx.alloc(&UpdateDatastoreMerkleRoot {
                    new_merkle_root: merkle_tree.root(),
                    memos,
                })?;
                (admin_key, Condition::Other(ptr))
            }
            _ => {
                return Err(Error::Custom(
                    "Exactly one of owner_synthetic_key or admin_synthetic_key must be provided."
                        .to_string(),
                ));
            }
        };

        let inner_spend = StandardLayer::new(inner_key)
            .spend_with_conditions(&mut ctx, Conditions::new().with(update_condition))?;
        let dl_spend = datastore.spend(&mut ctx, inner_spend)?;
        let new_datastore = SdkDatastore::<DatastoreMetadata>::from_spend(
            &mut ctx,
            &dl_spend,
            &parent_delegated_puzzles,
        )?
        .ok_or_else(|| {
            Error::Custom("Failed to parse Datastore after ownership update.".to_string())
        })?;
        ctx.insert(dl_spend);
        Ok(wrap_datastore(self.clvm.clone(), new_datastore))
    }
}

impl Clvm {
    /// Mint a `Datastore` via `Launcher::mint_datastore`.
    /// Returns parent conditions (apply on the funding coin) and the eve `Datastore`.
    pub fn mint_datastore(
        &self,
        parent_coin_id: Bytes32,
        metadata: DatastoreMetadata,
        owner_puzzle_hash: Bytes32,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Result<MintedDatastore> {
        let mut ctx = self.0.lock().unwrap();

        let (conditions, datastore) = Launcher::new(parent_coin_id, 1).mint_datastore(
            &mut ctx,
            metadata,
            owner_puzzle_hash.into(),
            delegated_puzzles,
        )?;

        let parent_conditions = conditions
            .into_iter()
            .map(|condition| Ok(Program(self.0.clone(), condition.to_clvm(&mut ctx)?)))
            .collect::<Result<Vec<_>>>()?;

        Ok(MintedDatastore {
            datastore: wrap_datastore(self.0.clone(), datastore),
            parent_conditions,
        })
    }

    /// Parse a `Datastore` child from a parent coin spend.
    /// When `delegated_puzzles` is `None`, uses `Oracle(zero, 0)` (slot-machine curated default).
    /// Pass `Some([...])` (including an empty list for vanilla stores) to override.
    pub fn datastore_from_spend(
        &self,
        parent_spend: CoinSpend,
        delegated_puzzles: Option<Vec<DelegatedPuzzle>>,
    ) -> Result<Option<Datastore>> {
        let mut ctx = self.0.lock().unwrap();
        let delegated_puzzles = delegated_puzzles.unwrap_or_else(oracle_delegated_puzzles);
        let Some(datastore) = SdkDatastore::<DatastoreMetadata>::from_spend(
            &mut ctx,
            &parent_spend,
            &delegated_puzzles,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(wrap_datastore(self.0.clone(), datastore)))
    }
}
