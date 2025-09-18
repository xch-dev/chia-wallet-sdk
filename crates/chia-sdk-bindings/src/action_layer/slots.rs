use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::Slot;
use chia_sdk_types::puzzles::{
    RewardDistributorCommitmentSlotValue, RewardDistributorEntrySlotValue,
    RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, SlotInfo,
};
use clvm_utils::ToTreeHash;

macro_rules! define_reward_distributor_slot {
    ($name:ident, $value_ty:ty, $nonce:expr, $ext_trait:ident) => {
        pub trait $ext_trait {}

        impl $ext_trait for $value_ty {}

        #[derive(Clone)]
        pub struct $name {
            pub coin: Coin,
            pub proof: LineageProof,

            pub nonce: u64,
            pub launcher_id: Bytes32,

            pub value: $value_ty,
        }

        impl $name {
            pub fn new(
                proof: LineageProof,
                launcher_id: Bytes32,
                value: $value_ty,
            ) -> Result<Self> {
                let slot = Slot::new(
                    proof,
                    SlotInfo::new(
                        launcher_id,
                        $nonce.to_u64(),
                        value.tree_hash().into(),
                        value,
                    ),
                );

                Ok(Self::from_slot(slot))
            }

            pub fn value_hash(&self) -> Result<Bytes32> {
                Ok(self.value.tree_hash().into())
            }

            pub fn to_slot(self) -> Slot<$value_ty> {
                Slot::new(
                    self.proof,
                    SlotInfo::new(
                        self.launcher_id,
                        self.nonce,
                        self.value.tree_hash().into(),
                        self.value,
                    ),
                )
            }

            pub fn from_slot(slot: Slot<$value_ty>) -> Self {
                Self {
                    coin: slot.coin,
                    proof: slot.proof,
                    nonce: slot.info.nonce,
                    launcher_id: slot.info.launcher_id,
                    value: slot.info.value,
                }
            }
        }
    };
}

define_reward_distributor_slot!(
    RewardSlot,
    RewardDistributorRewardSlotValue,
    RewardDistributorSlotNonce::REWARD,
    RewardDistributorRewardSlotValueExt
);

define_reward_distributor_slot!(
    CommitmentSlot,
    RewardDistributorCommitmentSlotValue,
    RewardDistributorSlotNonce::COMMITMENT,
    RewardDistributorCommitmentSlotValueExt
);

define_reward_distributor_slot!(
    EntrySlot,
    RewardDistributorEntrySlotValue,
    RewardDistributorSlotNonce::ENTRY,
    RewardDistributorEntrySlotValueExt
);
