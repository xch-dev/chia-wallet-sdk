use chia_protocol::Bytes32;
use chia_puzzle_types::{nft::NftStateLayerArgs, EveProof, Proof};
use chia_puzzles::NFT_STATE_LAYER_HASH;
use chia_sdk_types::{
    puzzles::{DelegationLayerArgs, DL_METADATA_UPDATER_PUZZLE_HASH},
    Conditions,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DriverError, Launcher, SpendContext};

use super::{get_merkle_tree, DataStore, DataStoreInfo, DelegatedPuzzle, DlLauncherKvList};

impl Launcher {
    pub fn mint_datastore<M>(
        self,
        ctx: &mut SpendContext,
        metadata: M,
        owner_puzzle_hash: TreeHash,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Result<(Conditions, DataStore<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let launcher_coin = self.coin();
        let launcher_id = launcher_coin.coin_id();

        let inner_puzzle_hash: TreeHash = if delegated_puzzles.is_empty() {
            owner_puzzle_hash
        } else {
            DelegationLayerArgs::curry_tree_hash(
                launcher_id,
                owner_puzzle_hash.into(),
                get_merkle_tree(ctx, delegated_puzzles.clone())?.root(),
            )
        };

        let metadata_ptr = ctx.alloc(&metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);
        let state_layer_hash = CurriedProgram {
            program: TreeHash::new(NFT_STATE_LAYER_HASH),
            args: NftStateLayerArgs::<TreeHash, TreeHash> {
                mod_hash: NFT_STATE_LAYER_HASH.into(),
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
        let kv_list = DlLauncherKvList {
            metadata: metadata.clone(),
            state_layer_inner_puzzle_hash: inner_puzzle_hash.into(),
            memos,
        };

        let (chained_spend, eve_coin) = self.spend(ctx, state_layer_hash.into(), kv_list)?;

        let proof = Proof::Eve(EveProof {
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

#[cfg(test)]
mod tests {
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_test::{BlsPair, Simulator};
    use rstest::rstest;

    use crate::{
        tests::{ByteSize, Description, Label, RootHash},
        DataStoreMetadata, StandardLayer,
    };

    use super::*;

    #[rstest]
    fn test_datastore_launch(
        #[values(true, false)] use_label: bool,
        #[values(true, false)] use_description: bool,
        #[values(true, false)] use_byte_size: bool,
        #[values(true, false)] with_writer: bool,
        #[values(true, false)] with_admin: bool,
        #[values(true, false)] with_oracle: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();

        let [owner, admin, writer] = BlsPair::range();

        let oracle_puzzle_hash: Bytes32 = [7; 32].into();
        let oracle_fee = 1000;

        let owner_puzzle_hash = StandardArgs::curry_tree_hash(owner.pk).into();
        let coin = sim.new_coin(owner_puzzle_hash, 1);

        let ctx = &mut SpendContext::new();

        let admin_delegated_puzzle =
            DelegatedPuzzle::Admin(StandardArgs::curry_tree_hash(admin.pk));
        let writer_delegated_puzzle =
            DelegatedPuzzle::Writer(StandardArgs::curry_tree_hash(writer.pk));
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
        StandardLayer::new(owner.pk).spend(ctx, coin, launch_singleton)?;

        let spends = ctx.take();
        for spend in spends.clone() {
            if spend.coin.coin_id() == datastore.info.launcher_id {
                let new_datastore = DataStore::from_spend(ctx, &spend, &[])?.unwrap();

                assert_eq!(datastore, new_datastore);
            }

            ctx.insert(spend);
        }

        assert_eq!(datastore.info.metadata, metadata);

        sim.spend_coins(spends, &[owner.sk, admin.sk, writer.sk])?;

        // Make sure the datastore was created.
        let coin_state = sim
            .coin_state(datastore.coin.coin_id())
            .expect("expected datastore coin");
        assert_eq!(coin_state.coin, datastore.coin);
        assert!(coin_state.created_height.is_some());

        Ok(())
    }
}
