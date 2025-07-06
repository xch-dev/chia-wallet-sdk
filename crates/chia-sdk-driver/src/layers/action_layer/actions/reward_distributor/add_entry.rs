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
    RewardDistributorSlotNonce, RewardDistributorState, Slot, SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorAddEntryAction {
    pub launcher_id: Bytes32,
    pub manager_launcher_id: Bytes32,
    pub max_second_offset: u64,
}

impl ToTreeHash for RewardDistributorAddEntryAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorAddEntryActionArgs::curry_tree_hash(
            self.launcher_id,
            self.manager_launcher_id,
            self.max_second_offset,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorAddEntryAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            manager_launcher_id: constants.manager_or_collection_did_launcher_id,
            max_second_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorAddEntryAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_add_entry_action_puzzle()?,
            args: RewardDistributorAddEntryActionArgs::new(
                self.launcher_id,
                self.manager_launcher_id,
                self.max_second_offset,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorAddEntryActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_payout_puzzle_hash,
            initial_cumulative_payout: state.round_reward_info.cumulative_payout,
            shares: solution.entry_shares,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        payout_puzzle_hash: Bytes32,
        shares: u64,
        manager_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Conditions, DriverError> {
        // calculate message that the manager needs to send
        let add_entry_message: Bytes32 = clvm_tuple!(payout_puzzle_hash, shares).tree_hash().into();
        let mut add_entry_message: Vec<u8> = add_entry_message.to_vec();
        add_entry_message.insert(0, b'a');
        let add_entry_message = Conditions::new().send_message(
            18,
            add_entry_message.into(),
            vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
        );

        // spend self
        let action_solution = ctx.alloc(&RewardDistributorAddEntryActionSolution {
            manager_singleton_inner_puzzle_hash,
            entry_payout_puzzle_hash: payout_puzzle_hash,
            entry_shares: shares,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;
        Ok(add_entry_message)
    }
}

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE: [u8; 590] = hex!("ff02ffff01ff04ffff04ff819fffff04ff82015fffff04ffff10ff8202dfff8203bf80ffff04ff8205dfffff04ff820bdfff808080808080ffff04ffff04ff1cffff04ffff0112ffff04ffff0effff0161ffff0bffff0102ffff0bffff0101ff8202bf80ffff0bffff0101ff8203bf808080ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff0b80ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff82013f80ffff0bff0aff66ff46808080ff46808080ff46808080ff8080808080ffff04ffff02ff1effff04ff02ffff04ff17ffff04ffff0bffff0102ffff0bffff0101ff8202bf80ffff0bffff0102ffff0bffff0101ff8209df80ffff0bffff0101ff8203bf808080ffff04ff8202bfff808080808080ffff04ffff04ff08ffff04ffff10ff8213dfff2f80ff808080ff8080808080ffff04ffff01ffff55ff3343ff02ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff14ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0580ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff0bffff0101ff0b8080ffff0bff0aff66ff46808080ff46808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ff018080");

pub const REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    90eef279e5389305ed3ff673fa5c766258e5ea04ff7abcec1ed551060bca8aa0
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorAddEntryActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub manager_singleton_struct_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
}

impl RewardDistributorAddEntryActionArgs {
    pub fn new(launcher_id: Bytes32, manager_launcher_id: Bytes32, max_second_offset: u64) -> Self {
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
            max_second_offset,
        }
    }
}

impl RewardDistributorAddEntryActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        manager_launcher_id: Bytes32,
        max_second_offset: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH,
            args: RewardDistributorAddEntryActionArgs::new(
                launcher_id,
                manager_launcher_id,
                max_second_offset,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorAddEntryActionSolution {
    pub manager_singleton_inner_puzzle_hash: Bytes32,
    pub entry_payout_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub entry_shares: u64,
}
