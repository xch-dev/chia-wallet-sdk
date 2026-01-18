use chia_protocol::Bytes32;
use chia_puzzle_types::{
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    singleton::SingletonStruct,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        NftLauncherProof, NonceWrapperArgs, P2DelegatedBySingletonLayerArgs,
        RewardDistributorCatLockingPuzzleSolution, RewardDistributorEntrySlotValue,
        RewardDistributorNftsFromDidLockingPuzzleSolution,
        RewardDistributorNftsFromDlLockingPuzzleSolution,
        RewardDistributorRefreshNftsFromDlActionArgs,
        RewardDistributorRefreshNftsFromDlActionSolution, RewardDistributorSlotNonce,
        RewardDistributorStakeActionSolution, SlotAndNfts, StakeNftFromDidInfo, StakeNftFromDlInfo,
        NONCE_WRAPPER_PUZZLE_HASH,
    },
    Conditions, MerkleProof, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    Asset, Cat, DriverError, HashedPtr, Nft, RewardDistributor, RewardDistributorConstants,
    RewardDistributorState, RewardDistributorType, SingletonAction, Slot, Spend, SpendContext,
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
                shares: e.existing_slot_value.shares + e.nfts_total_shares_delta,
            })
            .collect())
    }

    pub fn spent_slot_value(
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

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        slots_and_nfts: Vec<SlotAndNfts>,
        dl_root_hash: Bytes32,
        dl_metadata_rest_hash: Option<Bytes32>,
        dl_metadata_updater_hash_hash: Bytes32,
        dl_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Vec<Nft>), DriverError> {
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

        // if needed, spend existing slots
        if let Some(existing_slot) = existing_slot {
            let mut msg = (u128::from(existing_slot.info.value.shares)
                * (distributor
                    .pending_spend
                    .latest_state
                    .1
                    .round_reward_info
                    .cumulative_payout
                    - existing_slot.info.value.initial_cumulative_payout))
                .tree_hash()
                .to_vec();
            msg.insert(0, b's');
            security_conditions = security_conditions.send_message(
                18,
                msg.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            );
            existing_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;
        }

        // ensure new slot is properly created
        let new_slot_value = Self::created_slot_value(
            ctx,
            &distributor.pending_spend.latest_state.1,
            self.distributor_type,
            action_solution,
        )?;
        let mut msg = new_slot_value.tree_hash().to_vec();
        msg.insert(0, b't');
        security_conditions = security_conditions
            .assert_puzzle_announcement(announcement_id(distributor.coin.puzzle_hash, msg));
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((security_conditions, notarized_payments, created_nfts))
    }
}
