use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::Slot;
use chia_sdk_types::puzzles::SlotInfo;
use clvm_utils::ToTreeHash;

macro_rules! define_slot {
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
                    SlotInfo::new(launcher_id, $nonce, value.tree_hash().into(), value),
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

define_slot!(
    RewardSlot,
    chia_sdk_types::puzzles::RewardDistributorRewardSlotValue,
    chia_sdk_types::puzzles::RewardDistributorSlotNonce::REWARD.to_u64(),
    RewardDistributorRewardSlotValueExt
);

define_slot!(
    CommitmentSlot,
    chia_sdk_types::puzzles::RewardDistributorCommitmentSlotValue,
    chia_sdk_types::puzzles::RewardDistributorSlotNonce::COMMITMENT.to_u64(),
    RewardDistributorCommitmentSlotValueExt
);

define_slot!(
    EntrySlot,
    chia_sdk_types::puzzles::RewardDistributorEntrySlotValue,
    chia_sdk_types::puzzles::RewardDistributorSlotNonce::ENTRY.to_u64(),
    RewardDistributorEntrySlotValueExt
);

define_slot!(
    CatalogSlot,
    chia_sdk_types::puzzles::CatalogSlotValue,
    0u64,
    CatalogSlotValueRemoteExt
);

define_slot!(
    XchandlesHandleSlot,
    chia_sdk_types::puzzles::XchandlesHandleSlotValue,
    chia_sdk_types::puzzles::XchandlesSlotNonce::HANDLE.to_u64(),
    XchandlesHandleSlotValueRemoteExt
);

define_slot!(
    XchandlesUpdateSlot,
    chia_sdk_types::puzzles::XchandlesUpdateSlotValue,
    chia_sdk_types::puzzles::XchandlesSlotNonce::UPDATE.to_u64(),
    XchandlesUpdateSlotValueRemoteExt
);
