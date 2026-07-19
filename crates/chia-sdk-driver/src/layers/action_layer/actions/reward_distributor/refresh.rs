use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_puzzle_types::{nft::NftRoyaltyTransferPuzzleArgs, singleton::SingletonStruct};
use chia_sdk_types::{
    Conditions, MerkleProof, Mod, announcement_id,
    puzzles::{
        NONCE_WRAPPER_PUZZLE_HASH, NonceWrapperArgs, P2DelegatedBySingletonLayerArgs,
        P2DelegatedBySingletonLayerSolution, RefreshNftInfo, RewardDistributorDlInfo,
        RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue,
        RewardDistributorRefreshNftsFromDlActionArgs,
        RewardDistributorRefreshNftsFromDlActionSolution, RewardDistributorRefreshNftsTotals,
        RewardDistributorSlotNonce, SlotAndNfts,
    },
};
use clvm_traits::{clvm_quote, clvm_tuple};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, Layer, Nft, P2DelegatedBySingletonLayer, RewardDistributor,
    RewardDistributorConstants, RewardDistributorCreatedAnnouncementPrefix,
    RewardDistributorNftStakeEntry, RewardDistributorRefreshNftsFromDlActionLog,
    RewardDistributorStateTransition, RewardDistributorType, SingletonAction, Slot, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorRefreshAction {
    pub launcher_id: Bytes32,
    pub max_second_offset: u64,
    pub distributor_type: RewardDistributorType,
    pub precision: u64,
}

impl ToTreeHash for RewardDistributorRefreshAction {
    fn tree_hash(&self) -> TreeHash {
        if let Ok(args) = Self::new_args(
            self.launcher_id,
            self.max_second_offset,
            self.distributor_type,
            self.precision,
        ) {
            args.curry_tree_hash()
        } else {
            TreeHash::new([0; 32])
        }
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorRefreshAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            max_second_offset: constants.max_seconds_offset,
            distributor_type: constants.reward_distributor_type,
            precision: constants.precision,
        }
    }
}

impl RewardDistributorRefreshAction {
    pub fn new_args(
        launcher_id: Bytes32,
        max_second_offset: u64,
        distributor_type: RewardDistributorType,
        precision: u64,
    ) -> Result<RewardDistributorRefreshNftsFromDlActionArgs, DriverError> {
        match distributor_type {
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable,
            } => {
                if !refreshable {
                    return Err(DriverError::Custom(
                        "Refresh action is only available in *refreshable* curated NFT mode"
                            .to_string(),
                    ));
                }

                Ok(RewardDistributorRefreshNftsFromDlActionArgs::new(
                    store_launcher_id,
                    Self::my_p2_puzzle_hash(launcher_id),
                    Slot::<()>::first_curry_hash(
                        launcher_id,
                        RewardDistributorSlotNonce::ENTRY.to_u64(),
                    )
                    .into(),
                    max_second_offset,
                    precision,
                ))
            }
            _ => Err(DriverError::Custom(
                "Refresh action is only available in curated NFT mode".to_string(),
            )),
        }
    }

    pub fn my_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
        P2DelegatedBySingletonLayerArgs::curry_tree_hash(
            SingletonStruct::new(launcher_id).tree_hash().into(),
            1,
        )
        .into()
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let args = Self::new_args(
            self.launcher_id,
            self.max_second_offset,
            self.distributor_type,
            self.precision,
        )?;

        ctx.curry(args)
    }

    #[allow(clippy::cast_sign_loss)]
    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
        store_launcher_id: Bytes32,
    ) -> Result<RewardDistributorRefreshNftsFromDlActionLog, DriverError> {
        let params = ctx.extract::<RewardDistributorRefreshNftsFromDlActionSolution>(solution)?;

        let spent_entry_slots = params
            .slots_and_nfts
            .iter()
            .map(|e| e.existing_slot_value)
            .collect();
        let created_entry_slots = params
            .slots_and_nfts
            .iter()
            .map(|e| {
                Ok(RewardDistributorEntrySlotValue {
                    counter: e.existing_slot_value.counter + 1,
                    payout_puzzle_hash: e.existing_slot_value.payout_puzzle_hash,
                    initial_cumulative_payout: changes
                        .old_state
                        .round_reward_info
                        .cumulative_payout,
                    shares: u64::try_from(
                        i128::from(e.existing_slot_value.shares)
                            + i128::from(e.nfts_total_shares_delta),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, DriverError>>()?;
        let nft_entries = params
            .slots_and_nfts
            .iter()
            .flat_map(|slot| &slot.nfts)
            .map(|nft| RewardDistributorNftStakeEntry {
                launcher_id: nft.nft_launcher_id,
                shares: nft.new_nft_shares,
            })
            .collect();

        Ok(RewardDistributorRefreshNftsFromDlActionLog {
            spent_entry_slots,
            created_entry_slots,
            nft_entries,
            dl_root_hash: params.dl_root_hash,
            dl_inner_puzzle_hash: params.dl_info.dl_inner_puzzle_hash,
            dl_full_puzzle_hash: SingletonArgs::curry_tree_hash(
                store_launcher_id,
                params.dl_info.dl_inner_puzzle_hash.into(),
            )
            .into(),
            changes,
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_sign_loss)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        slots: Vec<Slot<RewardDistributorEntrySlotValue>>,
        nfts: &[&[Nft]],
        nft_shares_delta: &[&[i64]],
        nft_new_shares: &[&[u64]],
        nft_inclusion_proofs: &[&[MerkleProof]],
        dl_root_hash: Bytes32,
        dl_metadata_rest_hash: Option<Bytes32>,
        dl_metadata_updater_hash_hash: Bytes32,
        dl_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Vec<Nft>), DriverError> {
        // spend existing slots, build security conds, compute NFT children
        let mut security_conditions = Conditions::new();
        let mut slots_and_nfts = Vec::<SlotAndNfts>::new();
        let mut created_nfts = Vec::<Nft>::new();

        let my_inner_puzzle_hash: Bytes32 = distributor.info.inner_puzzle_hash().into();
        let my_p2_puzzle_hash = Self::my_p2_puzzle_hash(self.launcher_id);
        let my_p2_treehash: TreeHash = my_p2_puzzle_hash.into();
        let my_singleton_struct_hash = SingletonStruct::new(self.launcher_id).tree_hash().into();

        for (i, slot) in slots.into_iter().enumerate() {
            let slot = distributor.actual_entry_slot_value(slot);
            let mut nft_infos = Vec::<RefreshNftInfo>::new();
            for (j, nft) in nfts[i].iter().enumerate() {
                // add NFT data to solution
                nft_infos.push(RefreshNftInfo {
                    nft_shares_delta: nft_shares_delta[i][j],
                    new_nft_shares: nft_new_shares[i][j],
                    nft_parent_id: nft.coin.parent_coin_info,
                    nft_launcher_id: nft.info.launcher_id,
                    nft_metadata_hash: nft.info.metadata.tree_hash().into(),
                    nft_metadata_updater_hash_hash: nft
                        .info
                        .metadata_updater_puzzle_hash
                        .tree_hash()
                        .into(),
                    nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                        nft.info.launcher_id,
                        nft.info.royalty_puzzle_hash,
                        nft.info.royalty_basis_points,
                    )
                    .into(),
                    nft_owner: nft.info.current_owner,
                    nft_inclusion_proof: nft_inclusion_proofs[i][j].clone(),
                });

                // spend NFT
                let new_nft_inner_puzzle_hash = CurriedProgram {
                    program: NONCE_WRAPPER_PUZZLE_HASH,
                    args: NonceWrapperArgs::<(Bytes32, u64), TreeHash> {
                        nonce: clvm_tuple!(
                            slot.info.value.payout_puzzle_hash,
                            nft_new_shares[i][j]
                        ),
                        inner_puzzle: my_p2_treehash,
                    },
                }
                .tree_hash()
                .into();
                let nft_p2 = P2DelegatedBySingletonLayer::new(my_singleton_struct_hash, 1);
                let nft_inner_puzzle = nft_p2.construct_puzzle(ctx)?;
                let old_nft_shares = u64::try_from(
                    i128::from(nft_new_shares[i][j]) - i128::from(nft_shares_delta[i][j]),
                )?;
                let nft_nonce: (Bytes32, u64) =
                    clvm_tuple!(slot.info.value.payout_puzzle_hash, old_nft_shares);
                let nft_inner_puzzle = ctx.curry(NonceWrapperArgs::<(Bytes32, u64), NodePtr> {
                    nonce: nft_nonce,
                    inner_puzzle: nft_inner_puzzle,
                })?;

                let hint = ctx.hint(
                    (slot.info.value.payout_puzzle_hash, my_p2_puzzle_hash)
                        .tree_hash()
                        .into(),
                )?;
                let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
                    new_nft_inner_puzzle_hash,
                    1,
                    hint,
                )))?;
                let nft_inner_solution = nft_p2.construct_solution(
                    ctx,
                    P2DelegatedBySingletonLayerSolution::<NodePtr, NodePtr> {
                        singleton_inner_puzzle_hash: my_inner_puzzle_hash,
                        delegated_puzzle,
                        delegated_solution: NodePtr::NIL,
                    },
                )?;

                created_nfts
                    .push(nft.spend(ctx, Spend::new(nft_inner_puzzle, nft_inner_solution))?);

                // compute security condition for this NFT
                security_conditions =
                    security_conditions.assert_puzzle_announcement(announcement_id(
                        distributor.coin.puzzle_hash,
                        RewardDistributorCreatedAnnouncementPrefix::refresh(nft.info.launcher_id),
                    ));
            }

            let payout_amount_precision = u128::from(slot.info.value.shares)
                * (distributor
                    .pending_spend
                    .latest_state
                    .1
                    .round_reward_info
                    .cumulative_payout
                    - slot.info.value.initial_cumulative_payout);
            let entry_payout_amount =
                u64::try_from(payout_amount_precision / u128::from(self.precision))?;
            let payout_rounding_error = payout_amount_precision % u128::from(self.precision);
            slots_and_nfts.push(SlotAndNfts {
                existing_slot_value: slot.info.value,
                entry_payout_info: RewardDistributorEntryPayoutInfo {
                    payout_amount: entry_payout_amount,
                    payout_rounding_error,
                },
                nfts_total_shares_delta: nft_infos.iter().map(|e| e.nft_shares_delta).sum(),
                nfts: nft_infos,
            });
            slot.spend(ctx, my_inner_puzzle_hash)?;
        }

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorRefreshNftsFromDlActionSolution {
            dl_root_hash,
            dl_info: RewardDistributorDlInfo {
                dl_metadata_rest_hash,
                dl_metadata_updater_hash_hash,
                dl_inner_puzzle_hash,
            },
            totals: RewardDistributorRefreshNftsTotals {
                total_entry_payout_amount: slots_and_nfts
                    .iter()
                    .map(|e| e.entry_payout_info.payout_amount)
                    .sum(),
                total_shares_delta: i128::from(
                    slots_and_nfts
                        .iter()
                        .map(|e| e.nfts_total_shares_delta)
                        .sum::<i64>(),
                ),
                total_payout_rounding_error: slots_and_nfts
                    .iter()
                    .map(|e| e.entry_payout_info.payout_rounding_error)
                    .sum(),
            },
            slots_and_nfts,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((security_conditions, created_nfts))
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_types::puzzles::{
        RewardDistributorDlInfo, RewardDistributorEntryPayoutInfo,
        RewardDistributorRefreshNftsFromDlActionSolution, RewardDistributorRefreshNftsTotals,
    };

    use super::*;
    use crate::{
        RewardDistributorState, RewardDistributorStateTransition, RoundRewardInfo, RoundTimeInfo,
    };

    fn id(byte: u8) -> Bytes32 {
        Bytes32::new([byte; 32])
    }

    fn nft(launcher_id: Bytes32, shares_delta: i64, new_shares: u64) -> RefreshNftInfo {
        RefreshNftInfo {
            nft_shares_delta: shares_delta,
            new_nft_shares: new_shares,
            nft_parent_id: Bytes32::default(),
            nft_launcher_id: launcher_id,
            nft_metadata_hash: Bytes32::default(),
            nft_metadata_updater_hash_hash: Bytes32::default(),
            nft_transfer_porgram_hash: Bytes32::default(),
            nft_owner: None,
            nft_inclusion_proof: MerkleProof::new(0, vec![]),
        }
    }

    fn slot(shares: u64, shares_delta: i64, nfts: Vec<RefreshNftInfo>) -> SlotAndNfts {
        SlotAndNfts {
            existing_slot_value: RewardDistributorEntrySlotValue {
                counter: 0,
                payout_puzzle_hash: Bytes32::default(),
                initial_cumulative_payout: 0,
                shares,
            },
            entry_payout_info: RewardDistributorEntryPayoutInfo {
                payout_amount: 0,
                payout_rounding_error: 0,
            },
            nfts_total_shares_delta: shares_delta,
            nfts,
        }
    }

    #[test]
    fn refresh_log_maps_each_nft_to_its_new_shares_across_slot_groups() {
        let first = id(1);
        let second = id(2);
        let third = id(3);
        let mut ctx = SpendContext::new();
        let solution = ctx
            .alloc(&RewardDistributorRefreshNftsFromDlActionSolution {
                dl_root_hash: id(4),
                dl_info: RewardDistributorDlInfo {
                    dl_metadata_rest_hash: None,
                    dl_metadata_updater_hash_hash: id(5),
                    dl_inner_puzzle_hash: id(6),
                },
                totals: RewardDistributorRefreshNftsTotals {
                    total_entry_payout_amount: 0,
                    total_shares_delta: 1,
                    total_payout_rounding_error: 0,
                },
                slots_and_nfts: vec![
                    slot(12, -2, vec![nft(first, -4, 0), nft(second, 2, 2)]),
                    slot(7, 3, vec![nft(third, 3, 10)]),
                ],
            })
            .unwrap();
        let state = RewardDistributorState {
            total_reserves: 0,
            active_shares: 19,
            round_reward_info: RoundRewardInfo {
                cumulative_payout: 100,
                remaining_rewards: 0,
            },
            round_time_info: RoundTimeInfo {
                last_update: 0,
                epoch_end: 0,
            },
        };

        let log = RewardDistributorRefreshAction::get_log(
            &mut ctx,
            solution,
            RewardDistributorStateTransition {
                old_state: state,
                new_state: RewardDistributorState {
                    active_shares: 20,
                    ..state
                },
            },
            id(9),
        )
        .unwrap();

        assert_eq!(
            log.nft_entries,
            vec![
                RewardDistributorNftStakeEntry {
                    launcher_id: first,
                    shares: 0,
                },
                RewardDistributorNftStakeEntry {
                    launcher_id: second,
                    shares: 2,
                },
                RewardDistributorNftStakeEntry {
                    launcher_id: third,
                    shares: 10,
                },
            ]
        );
    }
}
