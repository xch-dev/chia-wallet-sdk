use chia_protocol::Bytes32;
use chia_puzzle_types::{nft::NftRoyaltyTransferPuzzleArgs, singleton::SingletonStruct};
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH,
    SINGLETON_TOP_LAYER_V1_1_HASH,
};
use chia_sdk_types::{
    puzzles::{
        NonceWrapperArgs, P2DelegatedBySingletonLayerArgs, P2DelegatedBySingletonLayerSolution,
        RewardDistributorEntrySlotValue, RewardDistributorSlotNonce,
        RewardDistributorUnstakeActionArgs, RewardDistributorUnstakeActionSolution,
        NONCE_WRAPPER_PUZZLE_HASH,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_quote;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    DriverError, Layer, Nft, P2DelegatedBySingletonLayer, RewardDistributor,
    RewardDistributorConstants, SingletonAction, Slot, Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorUnstakeAction {
    pub launcher_id: Bytes32,
    pub max_second_offset: u64,
}

impl ToTreeHash for RewardDistributorUnstakeAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args(self.launcher_id, self.max_second_offset).curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorUnstakeAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            max_second_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorUnstakeAction {
    pub fn new_args(
        launcher_id: Bytes32,
        max_second_offset: u64,
    ) -> RewardDistributorUnstakeActionArgs {
        RewardDistributorUnstakeActionArgs {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_hash: SINGLETON_LAUNCHER_HASH.into(),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
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
        ctx.curry(Self::new_args(self.launcher_id, self.max_second_offset))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorUnstakeActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_custody_puzzle_hash,
            initial_cumulative_payout: solution.entry_initial_cumulative_payout,
            shares: 1,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
        locked_nft: Nft,
    ) -> Result<(Conditions, u64), DriverError> {
        // u64 = last payment amount
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        // compute message that the custody puzzle needs to send
        let unstake_message = locked_nft.info.launcher_id.to_vec();

        let remove_entry_conditions = Conditions::new()
            .send_message(
                18,
                unstake_message.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(entry_slot.coin.puzzle_hash);

        // spend self
        let entry_payout_amount = entry_slot.info.value.shares
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let action_solution = ctx.alloc(&RewardDistributorUnstakeActionSolution {
            nft_launcher_id: locked_nft.info.launcher_id,
            nft_parent_id: locked_nft.coin.parent_coin_info,
            nft_metadata_hash: locked_nft.info.metadata.tree_hash().into(),
            nft_metadata_updater_hash_hash: locked_nft
                .info
                .metadata_updater_puzzle_hash
                .tree_hash()
                .into(),
            nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                locked_nft.info.launcher_id,
                locked_nft.info.royalty_puzzle_hash,
                locked_nft.info.royalty_basis_points,
            )
            .into(),
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_custody_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        let registry_inner_puzzle_hash = distributor.info.inner_puzzle_hash();
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend NFT
        let my_p2 = P2DelegatedBySingletonLayer::new(
            SingletonStruct::new(self.launcher_id).tree_hash().into(),
            1,
        );
        let nft_inner_puzzle = my_p2.construct_puzzle(ctx)?;
        // don't forget about the nonce wrapper!
        let nft_inner_puzzle = ctx.curry(NonceWrapperArgs::<Bytes32, NodePtr> {
            nonce: entry_slot.info.value.payout_puzzle_hash,
            inner_puzzle: nft_inner_puzzle,
        })?;

        let hint = ctx.hint(entry_slot.info.value.payout_puzzle_hash)?;
        let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
            entry_slot.info.value.payout_puzzle_hash,
            1,
            hint,
        )))?;
        let nft_inner_solution = my_p2.construct_solution(
            ctx,
            P2DelegatedBySingletonLayerSolution::<NodePtr, NodePtr> {
                singleton_inner_puzzle_hash: registry_inner_puzzle_hash.into(),
                delegated_puzzle,
                delegated_solution: NodePtr::NIL,
            },
        )?;

        let _new_nft = locked_nft.spend(ctx, Spend::new(nft_inner_puzzle, nft_inner_solution))?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        Ok((remove_entry_conditions, entry_payout_amount))
    }
}
