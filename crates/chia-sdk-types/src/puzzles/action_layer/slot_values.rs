use std::cmp::Ordering;

use chia_protocol::Bytes32;
use clvm_traits::{
    ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError, clvm_tuple,
};
use clvm_utils::ToTreeHash;
use hex_literal::hex;

/// Smallest 32-byte slot key, used as the left sentinel of a sorted slot list.
///
/// Slot keys are compared as unsigned byte strings (`>s`), so all zeros is the minimum.
// comparison is >s, not >
// previous min was 0x8000000000000000000000000000000000000000000000000000000000000000
// and previous max was 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
pub static SLOT32_MIN_VALUE: [u8; 32] =
    hex!("0000000000000000000000000000000000000000000000000000000000000000");
/// Largest 32-byte slot key, used as the right sentinel of a sorted slot list.
// the maximum possible value of a slot - will be contained by the other end of the list
pub static SLOT32_MAX_VALUE: [u8; 32] =
    hex!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");

/// Off-chain description of a slot: the controller, the slot type, and the stored value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct SlotInfo<V> {
    /// Slot type nonce (see [`Slot1stCurryArgs::nonce`](crate::puzzles::Slot1stCurryArgs::nonce)).
    pub nonce: u64,
    /// Launcher id of the controller singleton.
    pub launcher_id: Bytes32,

    /// Tree hash of [`value`](Self::value), curried into the slot puzzle.
    pub value_hash: Bytes32,
    /// Value stored in the slot.
    pub value: V,
}

impl<V> SlotInfo<V> {
    /// Creates the slot info with a precomputed value hash, which is not checked against
    /// `value`.
    pub fn new(launcher_id: Bytes32, nonce: u64, value_hash: Bytes32, value: V) -> Self {
        Self {
            nonce,
            launcher_id,
            value_hash,
            value,
        }
    }

    /// Creates the slot info, computing the value hash from `value`.
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

/// Keys of a slot's left and right neighbors in a sorted doubly linked slot list,
/// encoded as `(left_value . right_value)`.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotNeigborsInfo {
    /// Key of the previous slot in the list.
    pub left_value: Bytes32,
    /// Key of the next slot in the list.
    #[clvm(rest)]
    pub right_value: Bytes32,
}

/// Value of a `CATalog` slot, encoded as `(counter asset_id left_asset_id . right_asset_id)`.
///
/// Each registered CAT has a slot, and the slots form a list sorted by asset id between
/// [`initial_left_end`](Self::initial_left_end) and
/// [`initial_right_end`](Self::initial_right_end). Values are ordered by asset id.
///
/// # Example
///
/// ```rust
/// use chia_sdk_types::puzzles::{CatalogSlotValue, SLOT32_MAX_VALUE, SLOT32_MIN_VALUE};
///
/// let left = CatalogSlotValue::initial_left_end();
/// let right = CatalogSlotValue::initial_right_end();
///
/// assert_eq!(left.asset_id, SLOT32_MIN_VALUE.into());
/// assert_eq!(right.asset_id, SLOT32_MAX_VALUE.into());
/// assert!(left < right);
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogSlotValue {
    /// Number of times the slot has been spent and re-created. Bumping it on every spend
    /// makes each slot coin's puzzle hash unique.
    pub counter: u64,
    /// Asset id (TAIL hash) of the registered CAT, which is this slot's key.
    pub asset_id: Bytes32,
    /// Asset ids of the neighboring slots.
    #[clvm(rest)]
    pub neighbors: SlotNeigborsInfo,
}

impl CatalogSlotValue {
    /// Creates a slot value from the asset id and its neighbors' asset ids.
    pub fn new(
        counter: u64,
        asset_id: Bytes32,
        left_asset_id: Bytes32,
        right_asset_id: Bytes32,
    ) -> Self {
        Self {
            counter,
            asset_id,
            neighbors: SlotNeigborsInfo {
                left_value: left_asset_id,
                right_value: right_asset_id,
            },
        }
    }

    /// Sentinel slot at the left end of the list, created at launch: key
    /// [`SLOT32_MIN_VALUE`], with neighbors [`SLOT32_MIN_VALUE`] and [`SLOT32_MAX_VALUE`].
    pub fn initial_left_end() -> Self {
        Self::new(
            0,
            SLOT32_MIN_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
        )
    }

    /// Sentinel slot at the right end of the list, created at launch: key
    /// [`SLOT32_MAX_VALUE`], with neighbors [`SLOT32_MIN_VALUE`] and [`SLOT32_MAX_VALUE`].
    pub fn initial_right_end() -> Self {
        Self::new(
            0,
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

/// Slot nonces (types) used by the `XCHandles` registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XchandlesSlotNonce {
    /// Handle slots, with an [`XchandlesHandleSlotValue`].
    HANDLE = 1,
    /// Pending update slots, with an [`XchandlesUpdateSlotValue`].
    UPDATE = 2,
}

impl XchandlesSlotNonce {
    /// Returns the nonce with the given numeric value, if any.
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::HANDLE),
            2 => Some(Self::UPDATE),
            _ => None,
        }
    }

    /// Returns the numeric nonce curried into the slot puzzle.
    pub fn to_u64(self) -> u64 {
        match self {
            Self::HANDLE => 1,
            Self::UPDATE => 2,
        }
    }
}

/// Data attached to an `XCHandles` handle, encoded as
/// `(owner_launcher_id . resolved_launcher_id)`.
#[derive(ToClvm, FromClvm, Debug, Copy, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesDataValue {
    /// Launcher id of the singleton that owns the handle. It approves transfers and updates
    /// to the handle's data.
    pub owner_launcher_id: Bytes32,
    /// Launcher id of the singleton the handle resolves to.
    #[clvm(rest)]
    pub resolved_launcher_id: Bytes32,
}

/// Value of an `XCHandles` handle slot.
///
/// Encoded as `(counter (handle_hash left_value . right_value) expiration owner_launcher_id . resolved_launcher_id)`.
/// Handle slots form a list sorted by handle hash, and values are ordered by handle hash.
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::puzzles::{SLOT32_MAX_VALUE, SLOT32_MIN_VALUE, XchandlesHandleSlotValue};
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvm_utils::ToTreeHash;
/// use clvmr::Allocator;
///
/// let handle_hash: Bytes32 = "alice".to_string().tree_hash().into();
/// let owner = Bytes32::new([1; 32]);
///
/// let value = XchandlesHandleSlotValue::new(
///     0,
///     handle_hash,
///     SLOT32_MIN_VALUE.into(),
///     SLOT32_MAX_VALUE.into(),
///     1_700_000_000,
///     owner,
///     owner,
/// );
///
/// // Extending the registration re-creates the slot with a later expiration.
/// let extended = value.with_counter(1).with_expiration(1_731_622_400);
/// assert_eq!(extended.rest_data(), value.rest_data());
///
/// let mut allocator = Allocator::new();
/// let ptr = extended.to_clvm(&mut allocator)?;
/// assert_eq!(XchandlesHandleSlotValue::from_clvm(&allocator, ptr)?, extended);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesHandleSlotValue {
    /// Number of times the slot has been spent and re-created.
    pub counter: u64,
    /// Tree hash of the handle string, which is this slot's key.
    pub handle_hash: Bytes32,
    /// Handle hashes of the neighboring slots.
    pub neighbors: SlotNeigborsInfo,
    /// Timestamp, in seconds, at which the registration expires.
    pub expiration: u64,
    /// Launcher id of the singleton that owns the handle.
    pub owner_launcher_id: Bytes32,
    /// Launcher id of the singleton the handle resolves to.
    pub resolved_launcher_id: Bytes32,
}

impl XchandlesHandleSlotValue {
    /// Creates a handle slot value from its parts.
    pub fn new(
        counter: u64,
        handle_hash: Bytes32,
        left_handle_hash: Bytes32,
        right_handle_hash: Bytes32,
        expiration: u64,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Self {
        Self {
            counter,
            handle_hash,
            neighbors: SlotNeigborsInfo {
                left_value: left_handle_hash,
                right_value: right_handle_hash,
            },
            expiration,
            owner_launcher_id,
            resolved_launcher_id,
        }
    }

    /// Returns the owner and resolved launcher ids.
    pub fn rest_data(&self) -> XchandlesDataValue {
        XchandlesDataValue {
            owner_launcher_id: self.owner_launcher_id,
            resolved_launcher_id: self.resolved_launcher_id,
        }
    }

    /// Sentinel slot at the left end of the list: key [`SLOT32_MIN_VALUE`], neighbors
    /// [`SLOT32_MIN_VALUE`] and [`SLOT32_MAX_VALUE`], expiration [`u64::MAX`], and zeroed
    /// launcher ids.
    pub fn initial_left_end() -> Self {
        XchandlesHandleSlotValue::new(
            0,
            SLOT32_MIN_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
            u64::MAX,
            Bytes32::default(),
            Bytes32::default(),
        )
    }

    /// Sentinel slot at the right end of the list: key [`SLOT32_MAX_VALUE`], neighbors
    /// [`SLOT32_MIN_VALUE`] and [`SLOT32_MAX_VALUE`], expiration [`u64::MAX`], and zeroed
    /// launcher ids.
    pub fn initial_right_end() -> Self {
        XchandlesHandleSlotValue::new(
            0,
            SLOT32_MAX_VALUE.into(),
            SLOT32_MIN_VALUE.into(),
            SLOT32_MAX_VALUE.into(),
            u64::MAX,
            Bytes32::default(),
            Bytes32::default(),
        )
    }

    /// Returns a copy with the given neighbor handle hashes.
    #[must_use]
    pub fn with_neighbors(self, left_handle_hash: Bytes32, right_handle_hash: Bytes32) -> Self {
        Self {
            counter: self.counter,
            handle_hash: self.handle_hash,
            neighbors: SlotNeigborsInfo {
                left_value: left_handle_hash,
                right_value: right_handle_hash,
            },
            expiration: self.expiration,
            owner_launcher_id: self.owner_launcher_id,
            resolved_launcher_id: self.resolved_launcher_id,
        }
    }

    /// Returns a copy with the given expiration.
    #[must_use]
    pub fn with_expiration(self, expiration: u64) -> Self {
        Self {
            counter: self.counter,
            handle_hash: self.handle_hash,
            neighbors: self.neighbors,
            expiration,
            owner_launcher_id: self.owner_launcher_id,
            resolved_launcher_id: self.resolved_launcher_id,
        }
    }

    /// Returns a copy with the given owner and resolved launcher ids.
    #[must_use]
    pub fn with_data(self, owner_launcher_id: Bytes32, resolved_launcher_id: Bytes32) -> Self {
        Self {
            counter: self.counter,
            handle_hash: self.handle_hash,
            neighbors: self.neighbors,
            expiration: self.expiration,
            owner_launcher_id,
            resolved_launcher_id,
        }
    }

    /// Returns a copy with the given counter.
    #[must_use]
    pub fn with_counter(self, counter: u64) -> Self {
        Self {
            counter,
            handle_hash: self.handle_hash,
            neighbors: self.neighbors,
            expiration: self.expiration,
            owner_launcher_id: self.owner_launcher_id,
            resolved_launcher_id: self.resolved_launcher_id,
        }
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for XchandlesHandleSlotValue {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        #[allow(clippy::type_complexity)]
        let (
            counter,
            ((handle_hash, (left, right)), (expiration, (owner_launcher_id, resolved_launcher_id))),
        ): (
            u64,
            ((Bytes32, (Bytes32, Bytes32)), (u64, (Bytes32, Bytes32))),
        ) = FromClvm::from_clvm(decoder, node)?;

        Ok(Self::new(
            counter,
            handle_hash,
            left,
            right,
            expiration,
            owner_launcher_id,
            resolved_launcher_id,
        ))
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for XchandlesHandleSlotValue {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let obj = clvm_tuple!(
            self.counter,
            clvm_tuple!(
                clvm_tuple!(
                    self.handle_hash,
                    clvm_tuple!(self.neighbors.left_value, self.neighbors.right_value)
                ),
                clvm_tuple!(
                    self.expiration,
                    clvm_tuple!(self.owner_launcher_id, self.resolved_launcher_id)
                ),
            ),
        );

        obj.to_clvm(encoder)
    }
}

impl Ord for XchandlesHandleSlotValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.handle_hash.cmp(&other.handle_hash)
    }
}

impl PartialOrd for XchandlesHandleSlotValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Value of an `XCHandles` update slot, which records a pending change to a handle's
/// owner and resolved launcher ids.
///
/// Encoded as `((update_initiator_coin_id . min_height) handle_hash new_owner_launcher_id . new_resolved_launcher_id)`.
/// The initiate update action creates it, and the execute update action consumes it once
/// `min_height` is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XchandlesUpdateSlotValue {
    /// Coin id of the owner singleton coin that initiated the update. Its child must execute
    /// the update.
    pub update_initiator_coin_id: Bytes32,
    /// Block height at or after which the update can be executed.
    pub min_height: u32,
    /// Tree hash of the handle being updated.
    pub handle_hash: Bytes32,
    /// Launcher id of the new owner singleton.
    pub new_owner_launcher_id: Bytes32,
    /// Launcher id of the new resolved singleton.
    pub new_resolved_launcher_id: Bytes32,
}

impl XchandlesUpdateSlotValue {
    /// Creates an update slot value from its parts.
    pub fn new(
        update_initiator_coin_id: Bytes32,
        min_height: u32,
        handle_hash: Bytes32,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
    ) -> Self {
        Self {
            update_initiator_coin_id,
            min_height,
            handle_hash,
            new_owner_launcher_id,
            new_resolved_launcher_id,
        }
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for XchandlesUpdateSlotValue {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        #[allow(clippy::type_complexity)]
        let (
            (update_initiator_coin_id, min_height),
            (handle_hash, (new_owner_launcher_id, new_resolved_launcher_id)),
        ): ((Bytes32, u32), (Bytes32, (Bytes32, Bytes32))) = FromClvm::from_clvm(decoder, node)?;

        Ok(Self::new(
            update_initiator_coin_id,
            min_height,
            handle_hash,
            new_owner_launcher_id,
            new_resolved_launcher_id,
        ))
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for XchandlesUpdateSlotValue {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let obj = clvm_tuple!(
            clvm_tuple!(self.update_initiator_coin_id, self.min_height,),
            clvm_tuple!(
                self.handle_hash,
                clvm_tuple!(self.new_owner_launcher_id, self.new_resolved_launcher_id)
            ),
        );

        obj.to_clvm(encoder)
    }
}

/// Slot nonces (types) used by the reward distributor.
///
/// # Example
///
/// Compute the puzzle hash of a reward slot, which the distributor needs to create or
/// spend it:
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::{
///     Mod,
///     puzzles::{
///         RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, Slot1stCurryArgs,
///         Slot2ndCurryArgs,
///     },
/// };
/// use clvm_utils::{CurriedProgram, ToTreeHash};
///
/// let distributor_launcher_id = Bytes32::new([1; 32]);
/// let value = RewardDistributorRewardSlotValue {
///     counter: 0,
///     epoch_start: 1_700_000_000,
///     next_epoch_initialized: false,
///     rewards: 0,
/// };
///
/// let first_curry_hash = Slot1stCurryArgs::new(
///     distributor_launcher_id,
///     RewardDistributorSlotNonce::REWARD.to_u64(),
/// )
/// .curry_tree_hash();
/// let slot_puzzle_hash = CurriedProgram {
///     program: first_curry_hash,
///     args: Slot2ndCurryArgs {
///         value_hash: value.tree_hash().into(),
///     },
/// }
/// .tree_hash();
/// # let _ = slot_puzzle_hash;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardDistributorSlotNonce {
    /// Per-epoch reward slots, with a [`RewardDistributorRewardSlotValue`].
    REWARD = 1,
    /// Incentive commitment slots, with a [`RewardDistributorCommitmentSlotValue`].
    COMMITMENT = 2,
    /// Entry (share holder) slots, with a [`RewardDistributorEntrySlotValue`].
    ENTRY = 3,
    /// Deposit slots for staked assets, with a [`RewardDistributorDepositSlotValue`].
    DEPOSIT = 4,
}

impl RewardDistributorSlotNonce {
    /// Returns the nonce with the given numeric value, if any.
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::REWARD),
            2 => Some(Self::COMMITMENT),
            3 => Some(Self::ENTRY),
            4 => Some(Self::DEPOSIT),
            _ => None,
        }
    }

    /// Returns the numeric nonce curried into the slot puzzle.
    pub fn to_u64(self) -> u64 {
        match self {
            Self::REWARD => 1,
            Self::COMMITMENT => 2,
            Self::ENTRY => 3,
            Self::DEPOSIT => 4,
        }
    }
}

/// Value of a reward distributor reward slot, which tracks the rewards committed to one
/// epoch. Encoded as `(counter epoch_start next_epoch_initialized . rewards)`.
#[derive(
    ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
pub struct RewardDistributorRewardSlotValue {
    /// Number of times the slot has been spent and re-created.
    pub counter: u64,
    /// Start timestamp of the epoch, which is this slot's key.
    pub epoch_start: u64,
    /// Whether a reward slot for the following epoch has been created.
    pub next_epoch_initialized: bool,
    /// Total rewards committed to the epoch.
    #[clvm(rest)]
    pub rewards: u64,
}

/// Value of a reward distributor commitment slot, created when someone commits incentives
/// to a future epoch. Encoded as `(epoch_start clawback_ph . rewards)`.
#[derive(
    ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
pub struct RewardDistributorCommitmentSlotValue {
    /// Start timestamp of the epoch the incentives are committed to.
    pub epoch_start: u64,
    /// Puzzle hash that may withdraw the commitment before the epoch starts, and that
    /// receives the withdrawn amount.
    pub clawback_ph: Bytes32,
    /// Amount committed.
    #[clvm(rest)]
    pub rewards: u64,
}

/// Value of a reward distributor entry slot, which tracks one share holder.
/// Encoded as `(counter payout_puzzle_hash initial_cumulative_payout . shares)`.
///
/// The entry's pending reward is
/// `shares * (cumulative_payout - initial_cumulative_payout) / precision`, where
/// `cumulative_payout` comes from the distributor's state.
#[derive(
    ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
pub struct RewardDistributorEntrySlotValue {
    /// Number of times the slot has been spent and re-created.
    pub counter: u64,
    /// Puzzle hash that receives the entry's rewards. For staking distributors, it is also
    /// the custody puzzle hash that receives the unstaked assets.
    pub payout_puzzle_hash: Bytes32,
    /// The distributor's cumulative payout per share (scaled by the precision) when the
    /// entry was last paid out.
    pub initial_cumulative_payout: u128,
    /// Number of shares the entry holds.
    #[clvm(rest)]
    pub shares: u64,
}

/// Third field of a deposit slot: an NFT launcher id (32-byte atom) or a CAT amount
/// (CLVM integer).
///
/// Encodes as a raw atom so the on-chain `(payout . (shares . rest))` tree hash matches.
/// When parsing, a 32-byte atom is a launcher id and anything else is a CAT amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RewardDistributorDepositSlotAsset {
    /// Launcher id of a staked NFT.
    LauncherId(Bytes32),
    /// Amount of a staked CAT coin.
    CatAmount(u64),
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for RewardDistributorDepositSlotAsset {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let atom = decoder.decode_atom(&node)?;
        if atom.as_ref().len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(atom.as_ref());
            return Ok(Self::LauncherId(bytes.into()));
        }
        Ok(Self::CatAmount(u64::from_clvm(decoder, node)?))
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for RewardDistributorDepositSlotAsset {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        match self {
            Self::LauncherId(launcher_id) => launcher_id.to_clvm(encoder),
            Self::CatAmount(amount) => amount.to_clvm(encoder),
        }
    }
}

/// Value of a reward distributor deposit slot, which records one staked asset.
/// Encoded as `(payout_puzzle_hash shares . launcher_id_or_cat_amount)`.
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::puzzles::{
///     RewardDistributorDepositSlotAsset, RewardDistributorDepositSlotValue,
/// };
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvmr::Allocator;
///
/// let custody = Bytes32::new([1; 32]);
/// let mut allocator = Allocator::new();
///
/// // A staked CAT coin has one share per mojo.
/// let cat = RewardDistributorDepositSlotValue::cat(custody, 1_000);
/// assert_eq!(cat.shares, 1_000);
/// assert_eq!(
///     cat.launcher_id_or_cat_amount,
///     RewardDistributorDepositSlotAsset::CatAmount(1_000),
/// );
///
/// let nft = RewardDistributorDepositSlotValue::nft(custody, 1, Bytes32::new([2; 32]));
/// let ptr = nft.to_clvm(&mut allocator)?;
/// assert_eq!(RewardDistributorDepositSlotValue::from_clvm(&allocator, ptr)?, nft);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(
    ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
pub struct RewardDistributorDepositSlotValue {
    /// Custody puzzle hash of the entry the asset was staked for.
    pub payout_puzzle_hash: Bytes32,
    /// Shares the asset contributes to the entry.
    pub shares: u64,
    /// Staked NFT's launcher id, or the staked CAT coin's amount.
    #[clvm(rest)]
    pub launcher_id_or_cat_amount: RewardDistributorDepositSlotAsset,
}

impl RewardDistributorDepositSlotValue {
    /// Creates a deposit slot value from its parts.
    pub fn new(
        payout_puzzle_hash: Bytes32,
        shares: u64,
        launcher_id_or_cat_amount: RewardDistributorDepositSlotAsset,
    ) -> Self {
        Self {
            payout_puzzle_hash,
            shares,
            launcher_id_or_cat_amount,
        }
    }

    /// Creates the deposit slot value for a staked NFT.
    pub fn nft(payout_puzzle_hash: Bytes32, shares: u64, launcher_id: Bytes32) -> Self {
        Self::new(
            payout_puzzle_hash,
            shares,
            RewardDistributorDepositSlotAsset::LauncherId(launcher_id),
        )
    }

    /// Creates the deposit slot value for a staked CAT coin, with one share per mojo.
    pub fn cat(payout_puzzle_hash: Bytes32, cat_amount: u64) -> Self {
        Self::new(
            payout_puzzle_hash,
            cat_amount,
            RewardDistributorDepositSlotAsset::CatAmount(cat_amount),
        )
    }
}
