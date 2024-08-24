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

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use chia_bls::SecretKey;
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_secret_keys, test_transaction, Simulator};
    use rstest::rstest;

    use crate::{
        tests::{ByteSize, Description, Label, RootHash},
        DataStoreMetadata,
    };

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_datastore_launch(
        #[values(true, false)] use_label: bool,
        #[values(true, false)] use_description: bool,
        #[values(true, false)] use_byte_size: bool,
        #[values(true, false)] with_writer: bool,
        #[values(true, false)] with_admin: bool,
        #[values(true, false)] with_oracle: bool,
    ) -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let [owner_sk, admin_sk, writer_sk]: [SecretKey; 3] =
            test_secret_keys(3).unwrap().try_into().unwrap();

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
        if with_admin {
            delegated_puzzles.push(admin_delegated_puzzle);
        }
        if with_writer {
            delegated_puzzles.push(writer_delegated_puzzle);
        }
        if with_oracle {
            delegated_puzzles.push(oracle_delegated_puzzle);
        }

        let metadata = DataStoreMetadata {
            root_hash: RootHash::Zero.value(),
            label: if use_label { Label::Some.value() } else { None },
            description: if use_description {
                Description::Some.value()
            } else {
                None
            },
            bytes: if use_byte_size {
                ByteSize::Some.value()
            } else {
                None
            },
        };

        let (launch_singleton, datastore) = Launcher::new(coin.coin_id(), 1).mint_datastore(
            ctx,
            metadata.clone(),
            owner_puzzle_hash.into(),
            delegated_puzzles,
        )?;

        ctx.spend_p2_coin(coin, owner_pk, launch_singleton)?;

        let spends = ctx.take();
        for spend in spends.clone() {
            if spend.coin.coin_id() == datastore.info.launcher_id {
                let new_datastore =
                    DataStore::from_spend(&mut ctx.allocator, &spend, vec![])?.unwrap();

                assert_eq!(datastore, new_datastore);
            }

            ctx.insert(spend);
        }

        assert_eq!(datastore.info.metadata, metadata);

        test_transaction(
            &peer,
            spends,
            &[owner_sk, admin_sk, writer_sk],
            &sim.config().constants,
        )
        .await;

        // Make sure the datastore was created.
        let coin_state = sim
            .coin_state(datastore.coin.coin_id())
            .await
            .expect("expected datastore coin");
        assert_eq!(coin_state.coin, datastore.coin);
        assert!(coin_state.created_height.is_some());

        Ok(())
    }
}
