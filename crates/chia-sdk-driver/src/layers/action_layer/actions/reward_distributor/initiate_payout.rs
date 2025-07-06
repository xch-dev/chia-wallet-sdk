use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, RewardDistributor, RewardDistributorConstants, RewardDistributorEntrySlotValue,
    RewardDistributorSlotNonce, RewardDistributorState, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorInitiatePayoutAction {
    pub launcher_id: Bytes32,
    pub payout_threshold: u64,
}

impl ToTreeHash for RewardDistributorInitiatePayoutAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorInitiatePayoutAction::curry_tree_hash(
            self.launcher_id,
            self.payout_threshold,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorInitiatePayoutAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            payout_threshold: constants.payout_threshold,
        }
    }
}

impl RewardDistributorInitiatePayoutAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_initiate_payout_action_puzzle()?,
            args: RewardDistributorInitiatePayoutActionArgs::new(
                self.launcher_id,
                self.payout_threshold,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        current_state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorInitiatePayoutActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: current_state.round_reward_info.cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorInitiatePayoutActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: solution.entry_initial_cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
    ) -> Result<(Conditions, u64), DriverError> {
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        let withdrawal_amount = entry_slot.info.value.shares
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);

        // this announcement should be asserted to ensure everything goes according to plan
        let initiate_payout_announcement: Bytes32 =
            clvm_tuple!(entry_slot.info.value.payout_puzzle_hash, withdrawal_amount)
                .tree_hash()
                .into();
        let mut initiate_payout_announcement: Vec<u8> = initiate_payout_announcement.to_vec();
        initiate_payout_announcement.insert(0, b'p');

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorInitiatePayoutActionSolution {
            entry_payout_amount: withdrawal_amount,
            entry_payout_puzzle_hash: entry_slot.info.value.payout_puzzle_hash,
            entry_initial_cumulative_payout: entry_slot.info.value.initial_cumulative_payout,
            entry_shares: entry_slot.info.value.shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                initiate_payout_announcement,
            )),
            withdrawal_amount,
        ))
    }
}

impl RewardDistributorInitiatePayoutAction {
    pub fn curry_tree_hash(launcher_id: Bytes32, payout_threshold: u64) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH,
            args: RewardDistributorInitiatePayoutActionArgs::new(launcher_id, payout_threshold),
        }
        .tree_hash()
    }
}

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE: [u8; 724] = hex!("ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff820277ff82016f80ff8201ef80ff4f80ffff20ffff15ff0bff4f808080ffff01ff04ffff04ff27ffff04ffff11ff57ff4f80ff778080ffff04ffff02ff1effff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff81afffff04ff82016fffff04ff8201efff808080808080ff8080808080ffff04ffff02ff1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff81afffff04ff820277ffff04ff8201efff808080808080ffff04ff81afff808080808080ffff04ffff04ff18ffff04ffff0effff0170ffff0bffff0102ffff0bffff0101ff81af80ffff0bffff0101ff4f808080ff808080ffff04ffff04ffff0181d6ffff04ff10ffff04ff81afffff04ff4fffff04ffff04ff81afff8080ff808080808080ff808080808080ffff01ff088080ff0180ffff04ffff01ffffff333eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff10ffff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1cffff0bff72ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff1cff62ff42808080ff42808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80ffff0bffff0101ff17808080ff04ff14ffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1cffff0bff72ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff1cff62ff42808080ff42808080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ae41bf077dfbfdb93069d841dac67f8856a5637e45cefc9e1ecd00e0025266a9
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
}

impl RewardDistributorInitiatePayoutActionArgs {
    pub fn new(launcher_id: Bytes32, payout_threshold: u64) -> Self {
        Self {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            payout_threshold,
        }
    }
}

impl RewardDistributorInitiatePayoutActionArgs {
    pub fn curry_tree_hash(launcher_id: Bytes32, payout_threshold: u64) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH,
            args: RewardDistributorInitiatePayoutActionArgs::new(launcher_id, payout_threshold),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorInitiatePayoutActionSolution {
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u64,
    #[clvm(rest)]
    pub entry_shares: u64,
}
