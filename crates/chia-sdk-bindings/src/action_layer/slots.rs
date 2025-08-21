use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::Slot;
use chia_sdk_types::puzzles::{
    RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, SlotInfo,
};
use clvm_utils::ToTreeHash;

pub struct RewardSlot {
    pub coin: Coin,
    pub proof: LineageProof,

    pub nonce: u64,
    pub launcher_id: Bytes32,

    pub value: RewardDistributorRewardSlotValue,
}

impl RewardSlot {
    pub fn new(
        proof: LineageProof,
        launcher_id: Bytes32,
        value: RewardDistributorRewardSlotValue,
    ) -> Result<Self> {
        let slot = Slot::new(
            proof,
            SlotInfo::new(
                launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
                value.tree_hash().into(),
                value,
            ),
        );

        Ok(Self::from_slot(slot))
    }

    pub fn value_hash(&self) -> Result<Bytes32> {
        Ok(self.value.tree_hash().into())
    }

    pub fn to_slot(self) -> Slot<RewardDistributorRewardSlotValue> {
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

    pub fn from_slot(slot: Slot<RewardDistributorRewardSlotValue>) -> Self {
        Self {
            coin: slot.coin,
            proof: slot.proof,
            nonce: slot.info.nonce,
            launcher_id: slot.info.launcher_id,
            value: slot.info.value,
        }
    }
}
