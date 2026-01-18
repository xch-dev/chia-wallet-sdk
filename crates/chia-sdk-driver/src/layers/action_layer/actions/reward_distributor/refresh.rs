use chia_protocol::Bytes32;
use chia_puzzle_types::{nft::NftRoyaltyTransferPuzzleArgs, singleton::SingletonStruct};
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        NonceWrapperArgs, P2DelegatedBySingletonLayerArgs, P2DelegatedBySingletonLayerSolution,
        RefreshNftInfo, RewardDistributorEntrySlotValue,
        RewardDistributorRefreshNftsFromDlActionArgs,
        RewardDistributorRefreshNftsFromDlActionSolution, RewardDistributorSlotNonce, SlotAndNfts,
        NONCE_WRAPPER_PUZZLE_HASH,
    },
    Conditions, MerkleProof, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, clvm_tuple};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{serde::node_to_bytes, NodePtr};

use crate::{
    DriverError, Layer, Nft, P2DelegatedBySingletonLayer, RewardDistributor,
    RewardDistributorConstants, RewardDistributorState, RewardDistributorType, SingletonAction,
    Slot, Spend, SpendContext,
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
    pub fn created_slot_values(
        ctx: &mut SpendContext,
        state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<Vec<RewardDistributorEntrySlotValue>, DriverError> {
        let solution = ctx.extract::<RewardDistributorRefreshNftsFromDlActionSolution>(solution)?;

        Ok(solution
            .slots_and_nfts
            .iter()
            .map(|e| RewardDistributorEntrySlotValue {
                payout_puzzle_hash: e.existing_slot_value.payout_puzzle_hash,
                initial_cumulative_payout: state.round_reward_info.cumulative_payout,
                shares: if e.nfts_total_shares_delta > 0 {
                    e.existing_slot_value.shares + e.nfts_total_shares_delta as u64
                } else {
                    e.existing_slot_value.shares - (-e.nfts_total_shares_delta) as u64
                },
            })
            .collect())
    }

    pub fn spent_slot_values(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<Vec<RewardDistributorEntrySlotValue>, DriverError> {
        let solution = ctx.extract::<RewardDistributorRefreshNftsFromDlActionSolution>(solution)?;

        Ok(solution
            .slots_and_nfts
            .iter()
            .map(|e| e.existing_slot_value)
            .collect())
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
        let my_p2_treehash = Self::my_p2_puzzle_hash(self.launcher_id).into();
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
                let old_nft_shares = if nft_shares_delta[i][j] > 0 {
                    nft_new_shares[i][j] + (nft_shares_delta[i][j] as u64)
                } else {
                    nft_new_shares[i][j] - ((-nft_shares_delta[i][j]) as u64)
                };
                let nft_nonce: (Bytes32, u64) =
                    clvm_tuple!(slot.info.value.payout_puzzle_hash, old_nft_shares);
                let nft_inner_puzzle = ctx.curry(NonceWrapperArgs::<(Bytes32, u64), NodePtr> {
                    nonce: nft_nonce,
                    inner_puzzle: nft_inner_puzzle,
                })?;

                let hint = ctx.hint(new_nft_inner_puzzle_hash)?;
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
                let mut msg: Vec<u8> = nft.info.launcher_id.into();
                msg.insert(0, b'r');
                security_conditions = security_conditions
                    .assert_puzzle_announcement(announcement_id(distributor.coin.puzzle_hash, msg));
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
            let payout_rounding_error =
                u64::try_from(payout_amount_precision % u128::from(self.precision))?;
            slots_and_nfts.push(SlotAndNfts {
                existing_slot_value: slot.info.value,
                entry_payout_amount,
                payout_rounding_error,
                nfts_total_shares_delta: nft_infos.iter().map(|e| e.nft_shares_delta).sum(),
                nfts: nft_infos,
            });
            slot.spend(ctx, my_inner_puzzle_hash)?;
        }

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorRefreshNftsFromDlActionSolution {
            dl_root_hash,
            dl_metadata_rest_hash,
            dl_metadata_updater_hash_hash,
            dl_inner_puzzle_hash,
            total_entry_payout_amount: slots_and_nfts.iter().map(|e| e.entry_payout_amount).sum(),
            total_shares_delta: slots_and_nfts
                .iter()
                .map(|e| e.nfts_total_shares_delta)
                .sum(),
            total_payout_rounding_error: slots_and_nfts
                .iter()
                .map(|e| e.payout_rounding_error)
                .sum(),
            slots_and_nfts,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // todo: debug -------------------------
        println!(
            "action puzzle: {:?}",
            hex::encode(node_to_bytes(ctx, action_puzzle)?)
        );
        let actual_solution = ctx.alloc(&clvm_list!(
            distributor.pending_spend.latest_state,
            action_solution
        ))?;
        println!(
            "actual solution: {:?}",
            hex::encode(node_to_bytes(ctx, actual_solution)?)
        );
        // todo: debug -------------------------
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((security_conditions, created_nfts))
    }
}
