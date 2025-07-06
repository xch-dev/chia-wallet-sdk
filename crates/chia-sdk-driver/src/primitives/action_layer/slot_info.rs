use std::cmp::Ordering;

use chia::{
    clvm_utils::ToTreeHash,
    protocol::{Bytes, Bytes32},
};
use clvm_traits::{
    clvm_tuple, ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError,
};
use hex_literal::hex;

// comparison is >s, not >
// previous min was 0x8000000000000000000000000000000000000000000000000000000000000000
// and previous max was 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
pub static SLOT32_MIN_VALUE: [u8; 32] =
    hex!("0000000000000000000000000000000000000000000000000000000000000000");
// the maximum possible value of a slot - will be contained by the other end of the list
pub static SLOT32_MAX_VALUE: [u8; 32] =
    hex!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct SlotInfo<V> {
    pub nonce: u64,
    pub launcher_id: Bytes32,

    pub value_hash: Bytes32,
    pub value: V,
}

impl<V> SlotInfo<V> {
    pub fn new(launcher_id: Bytes32, nonce: u64, value_hash: Bytes32, value: V) -> Self {
        Self {
            launcher_id,
            nonce,
            value_hash,
            value,
        }
    }

    pub fn from_value(launcher_id: Bytes32, nonce: u64, value: V) -> Self
    where
        V: ToTreeHash,
    {
        Self {
            launcher_id,
            nonce,
            value_hash: value.tree_hash().into(),
            value,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotNeigborsInfo {
    pub left_value: Bytes32,
    #[clvm(rest)]
    pub right_value: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogSlotValue {
    pub asset_id: Bytes32,
    #[clvm(rest)]
    pub neighbors: SlotNeigborsInfo,
}

impl CatalogSlotValue {
    pub fn new(asset_id: Bytes32, left_asset_id: Bytes32, right_asset_id: Bytes32) -> Self {
        Self {
            asset_id,
            neighbors: SlotNeigborsInfo {
                left_value: left_asset_id,
                right_value: right_asset_id,
            },
        }
    }

    pub fn initial_left_end() -> Self {
        Self::new(
            SLOT32_MIN_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
        )
    }

    pub fn initial_right_end() -> Self {
        Self::new(
            SLOT32_MAX_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
        )
    }
}

impl Ord for CatalogSlotValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.asset_id.cmp(&other.asset_id)
    }
}

impl PartialOrd for CatalogSlotValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesDataValue {
    pub owner_launcher_id: Bytes32,
    #[clvm(rest)]
    pub resolved_data: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesSlotValue {
    pub handle_hash: Bytes32,
    pub neighbors: SlotNeigborsInfo,
    pub expiration: u64,
    pub owner_launcher_id: Bytes32,
    pub resolved_data: Bytes,
}

impl XchandlesSlotValue {
    pub fn new(
        handle_hash: Bytes32,
        left_handle_hash: Bytes32,
        right_handle_hash: Bytes32,
        expiration: u64,
        owner_launcher_id: Bytes32,
        resolved_data: Bytes,
    ) -> Self {
        Self {
            handle_hash,
            neighbors: SlotNeigborsInfo {
                left_value: left_handle_hash,
                right_value: right_handle_hash,
            },
            expiration,
            owner_launcher_id,
            resolved_data,
        }
    }

    pub fn rest_data(&self) -> XchandlesDataValue {
        XchandlesDataValue {
            owner_launcher_id: self.owner_launcher_id,
            resolved_data: self.resolved_data.clone(),
        }
    }

    pub fn initial_left_end() -> Self {
        XchandlesSlotValue::new(
            SLOT32_MIN_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
            u64::MAX,
            Bytes32::default(),
            Bytes::default(),
        )
    }

    pub fn initial_right_end() -> Self {
        XchandlesSlotValue::new(
            SLOT32_MAX_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
            u64::MAX,
            Bytes32::default(),
            Bytes::default(),
        )
    }

    pub fn with_neighbors(self, left_handle_hash: Bytes32, right_handle_hash: Bytes32) -> Self {
        Self {
            handle_hash: self.handle_hash,
            neighbors: SlotNeigborsInfo {
                left_value: left_handle_hash,
                right_value: right_handle_hash,
            },
            expiration: self.expiration,
            owner_launcher_id: self.owner_launcher_id,
            resolved_data: self.resolved_data,
        }
    }

    pub fn with_expiration(self, expiration: u64) -> Self {
        Self {
            handle_hash: self.handle_hash,
            neighbors: self.neighbors,
            expiration,
            owner_launcher_id: self.owner_launcher_id,
            resolved_data: self.resolved_data.clone(),
        }
    }

    pub fn with_data(self, owner_launcher_id: Bytes32, resolved_data: Bytes) -> Self {
        Self {
            handle_hash: self.handle_hash,
            neighbors: self.neighbors,
            expiration: self.expiration,
            owner_launcher_id,
            resolved_data,
        }
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for XchandlesSlotValue {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        #[allow(clippy::type_complexity)]
        let ((handle_hash, (left, right)), (expiration, (owner_launcher_id, resolved_data))): (
            (Bytes32, (Bytes32, Bytes32)),
            (u64, (Bytes32, Bytes)),
        ) = FromClvm::from_clvm(decoder, node)?;

        Ok(Self::new(
            handle_hash,
            left,
            right,
            expiration,
            owner_launcher_id,
            resolved_data,
        ))
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for XchandlesSlotValue {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let obj = clvm_tuple!(
            clvm_tuple!(
                self.handle_hash,
                clvm_tuple!(self.neighbors.left_value, self.neighbors.right_value)
            ),
            clvm_tuple!(
                self.expiration,
                clvm_tuple!(self.owner_launcher_id, self.resolved_data.clone())
            ),
        );

        obj.to_clvm(encoder)
    }
}

impl Ord for XchandlesSlotValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.handle_hash.cmp(&other.handle_hash)
    }
}

impl PartialOrd for XchandlesSlotValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardDistributorSlotNonce {
    REWARD = 1,
    COMMITMENT = 2,
    ENTRY = 3,
}

impl RewardDistributorSlotNonce {
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::REWARD),
            2 => Some(Self::COMMITMENT),
            3 => Some(Self::ENTRY),
            _ => None,
        }
    }

    pub fn to_u64(self) -> u64 {
        match self {
            Self::REWARD => 1,
            Self::COMMITMENT => 2,
            Self::ENTRY => 3,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRewardSlotValue {
    pub epoch_start: u64,
    pub next_epoch_initialized: bool,
    #[clvm(rest)]
    pub rewards: u64,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCommitmentSlotValue {
    pub epoch_start: u64,
    pub clawback_ph: Bytes32,
    #[clvm(rest)]
    pub rewards: u64,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorEntrySlotValue {
    pub payout_puzzle_hash: Bytes32,
    pub initial_cumulative_payout: u64,
    #[clvm(rest)]
    pub shares: u64,
}
