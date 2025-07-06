use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::Conditions,
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, RewardDistributor, RewardDistributorConstants, RewardDistributorEntrySlotValue,
    RewardDistributorSlotNonce, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorRemoveEntryAction {
    pub launcher_id: Bytes32,
    pub manager_launcher_id: Bytes32,
    pub max_seconds_offset: u64,
}

impl ToTreeHash for RewardDistributorRemoveEntryAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorRemoveEntryActionArgs::curry_tree_hash(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_seconds_offset,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorRemoveEntryAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            manager_launcher_id: constants.manager_or_collection_did_launcher_id,
            max_seconds_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorRemoveEntryAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_remove_entry_action_puzzle()?,
            args: RewardDistributorRemoveEntryActionArgs::new(
                self.launcher_id,
                self.manager_launcher_id,
                self.max_seconds_offset,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, u64), DriverError> {
        // u64 = last payment amount
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        // compute message that the manager needs to send
        let remove_entry_message: Bytes32 = clvm_tuple!(
            entry_slot.info.value.payout_puzzle_hash,
            entry_slot.info.value.shares
        )
        .tree_hash()
        .into();
        let mut remove_entry_message: Vec<u8> = remove_entry_message.to_vec();
        remove_entry_message.insert(0, b'r');

        let remove_entry_conditions = Conditions::new()
            .send_message(
                18,
                remove_entry_message.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(entry_slot.coin.puzzle_hash);

        // spend self
        let entry_payout_amount = entry_slot.info.value.shares
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let action_solution = ctx.alloc(&RewardDistributorRemoveEntryActionSolution {
            manager_singleton_inner_puzzle_hash,
            entry_payout_amount,
            entry_payout_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_shares: entry_slot.info.value.shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok((remove_entry_conditions, entry_payout_amount))
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorRemoveEntryActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: solution.entry_initial_cumulative_payout,
            shares: solution.entry_shares,
        })
    }
}

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE: [u8; 671] = hex!("ff02ffff01ff02ffff03ffff09ff8202bfffff12ffff11ff8209dfff820bbf80ff820fbf8080ffff01ff04ffff04ff819fffff04ffff11ff82015fff8202bf80ffff04ffff11ff8202dfff820fbf80ff8203df808080ffff04ffff04ff1cffff04ffff0112ffff04ffff0effff0172ffff0bffff0102ffff0bffff0101ff8205bf80ffff0bffff0101ff820fbf808080ffff04ffff0bff56ffff0bff1affff0bff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff0b80ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ff82013f80ffff0bff1aff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff04ff08ffff04ffff10ff8213dfff2f80ff808080ffff04ffff02ff1effff04ff02ffff04ff17ffff04ffff0bffff0102ffff0bffff0101ff8205bf80ffff0bffff0102ffff0bffff0101ff820bbf80ffff0bffff0101ff820fbf808080ff8080808080ffff04ffff04ffff0181d6ffff04ff14ffff04ff8205bfffff04ff8202bfffff04ffff04ff8205bfff8080ff808080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffff55ff3343ffff4202ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff12ffff04ffff0112ffff04ff80ffff04ffff0bff56ffff0bff1affff0bff1aff66ff0580ffff0bff1affff0bff76ffff0bff1affff0bff1aff66ffff0bffff0101ff0b8080ffff0bff1aff66ff46808080ff46808080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    4cb611d7003037ead2cf96a08989f0f063db05dfc548fc68cee23fbcd6887bed
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRemoveEntryActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub manager_singleton_struct_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_seconds_offset: u64,
}

impl RewardDistributorRemoveEntryActionArgs {
    pub fn new(
        launcher_id: Bytes32,
        manager_launcher_id: Bytes32,
        max_seconds_offset: u64,
    ) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            manager_singleton_struct_hash: SingletonStruct::new(manager_launcher_id)
                .tree_hash()
                .into(),
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_seconds_offset,
        }
    }
}

impl RewardDistributorRemoveEntryActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        manager_launcher_id: Bytes32,
        max_seconds_offset: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH,
            args: RewardDistributorRemoveEntryActionArgs::new(
                launcher_id,
                manager_launcher_id,
                max_seconds_offset,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRemoveEntryActionSolution {
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u64,
    #[clvm(rest)]
    pub entry_shares: u64,
}
