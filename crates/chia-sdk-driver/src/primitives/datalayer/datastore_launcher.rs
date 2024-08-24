use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{NftStateLayerArgs, NFT_STATE_LAYER_PUZZLE_HASH},
    EveProof, Proof,
};
use chia_sdk_types::Conditions;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    DelegationLayerArgs, DriverError, Launcher, SpendContext, DL_METADATA_UPDATER_PUZZLE_HASH,
};

use super::{get_merkle_tree, DLLauncherKVList, DataStore, DataStoreInfo, DelegatedPuzzle};

impl Launcher {
    pub fn mint_datastore<M>(
        self,
        ctx: &mut SpendContext,
        metadata: M,
        owner_puzzle_hash: TreeHash,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Result<(Conditions, DataStore<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
    {
        let launcher_coin = self.coin();
        let launcher_id = launcher_coin.coin_id();

        let inner_puzzle_hash: TreeHash = if delegated_puzzles.is_empty() {
            owner_puzzle_hash
        } else {
            DelegationLayerArgs::curry_tree_hash(
                launcher_id,
                owner_puzzle_hash.into(),
                get_merkle_tree(ctx, delegated_puzzles.clone())?.root,
            )
        };

        let metadata_hash = metadata.tree_hash();
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

        let mut memos = DataStore::<M>::get_recreation_memos(
            Bytes32::default(),
            owner_puzzle_hash,
            delegated_puzzles.clone(),
        )
        .into_iter()
        .skip(1)
        .collect();
        if delegated_puzzles.is_empty() {
            memos = vec![];
        }
        let kv_list = DLLauncherKVList {
            metadata: metadata.clone(),
            state_layer_inner_puzzle_hash: inner_puzzle_hash.into(),
            memos,
        };

        let (chained_spend, eve_coin) = self.spend(ctx, state_layer_hash.into(), kv_list)?;

        let proof: Proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        let data_store = DataStore {
            coin: eve_coin,
            proof,
            info: DataStoreInfo {
                launcher_id,
                metadata,
                owner_puzzle_hash: owner_puzzle_hash.into(),
                delegated_puzzles,
            },
        };

        Ok((chained_spend, data_store))
    }
}
