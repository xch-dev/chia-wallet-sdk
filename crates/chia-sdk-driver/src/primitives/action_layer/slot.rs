use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{LineageProof, singleton::SingletonArgs};
use chia_sdk_types::{
    Mod,
    puzzles::{Slot1stCurryArgs, Slot2ndCurryArgs, SlotInfo, SlotSolution},
};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

/// A 0-amount coin that stores a single value on behalf of a controller singleton.
///
/// Action-layer singletons (such as [`CatalogRegistry`](crate::CatalogRegistry) and
/// [`XchandlesRegistry`](crate::XchandlesRegistry)) use slots to keep data outside of their own
/// puzzle. The slot puzzle is curried in two steps: first with the controller singleton's launcher
/// id and a `nonce` that separates different kinds of slots, then with the tree hash of the value.
///
/// A slot must be created by its controller singleton, and can only be spent when the controller
/// singleton sends it an empty message in the same block. Spending a slot removes the value. The
/// controller usually creates a new slot with the updated value at the same time.
///
/// # Example
///
/// Compute the slot that a singleton spend creates:
///
/// ```
/// use chia_protocol::{Bytes32, Coin};
/// use chia_puzzle_types::{LineageProof, singleton::SingletonArgs};
/// use chia_sdk_driver::Slot;
/// use chia_sdk_types::puzzles::SlotInfo;
///
/// let launcher_id = Bytes32::new([1; 32]);
/// let parent_inner_puzzle_hash = Bytes32::new([2; 32]);
///
/// // The singleton coin that creates the slot.
/// let singleton_coin = Coin::new(
///     Bytes32::new([3; 32]),
///     SingletonArgs::curry_tree_hash(launcher_id, parent_inner_puzzle_hash.into()).into(),
///     1,
/// );
///
/// let info = SlotInfo::from_value(launcher_id, 0, 42_u64);
/// let slot = Slot::new(
///     LineageProof {
///         parent_parent_coin_info: singleton_coin.parent_coin_info,
///         parent_inner_puzzle_hash,
///         parent_amount: singleton_coin.amount,
///     },
///     info.clone(),
/// );
///
/// assert_eq!(slot.coin.parent_coin_info, singleton_coin.coin_id());
/// assert_eq!(slot.coin.puzzle_hash, Slot::puzzle_hash(&info).into());
/// assert_eq!(slot.coin.amount, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Slot<V> {
    /// The slot coin. Its amount is always 0.
    pub coin: Coin,
    /// The lineage proof of the controller singleton coin that created this slot.
    pub proof: LineageProof,

    /// The controller launcher id, nonce, and value stored in the slot.
    pub info: SlotInfo<V>,
}

impl<V> Slot<V> {
    /// Creates a [`Slot`] from the lineage proof of the singleton coin that created it.
    ///
    /// The coin's parent id is computed from `proof` and the controller launcher id in `info`.
    pub fn new(proof: LineageProof, info: SlotInfo<V>) -> Self {
        let parent_coin_id = Coin::new(
            proof.parent_parent_coin_info,
            SingletonArgs::curry_tree_hash(info.launcher_id, proof.parent_inner_puzzle_hash.into())
                .into(),
            proof.parent_amount,
        )
        .coin_id();

        Self {
            coin: Coin::new(parent_coin_id, Slot::<V>::puzzle_hash(&info).into(), 0),
            proof,
            info,
        }
    }

    /// Returns the tree hash of the slot puzzle curried with only the controller launcher id and nonce.
    ///
    /// Actions curry this hash so they can recreate slot puzzle hashes for any value. It is also
    /// used as the hint for slots created by registry launches.
    pub fn first_curry_hash(launcher_id: Bytes32, nonce: u64) -> TreeHash {
        Slot1stCurryArgs::new(launcher_id, nonce).curry_tree_hash()
    }

    /// Returns the full puzzle hash of a slot with the given launcher id, nonce, and value hash.
    pub fn puzzle_hash(info: &SlotInfo<V>) -> TreeHash {
        CurriedProgram {
            program: Self::first_curry_hash(info.launcher_id, info.nonce),
            args: Slot2ndCurryArgs {
                value_hash: info.value_hash,
            },
        }
        .tree_hash()
    }

    /// Allocates the slot puzzle.
    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let self_program = ctx.curry(Slot1stCurryArgs::new(
            self.info.launcher_id,
            self.info.nonce,
        ))?;

        ctx.alloc(&CurriedProgram {
            program: self_program,
            args: Slot2ndCurryArgs {
                value_hash: self.info.value_hash,
            },
        })
    }

    /// Spends the slot.
    ///
    /// `spender_inner_puzzle_hash` is the inner puzzle hash of the controller singleton coin being
    /// spent in the same block. That singleton must send this slot an empty message, which the
    /// action that consumes the slot normally does.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        spender_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let puzzle_reveal = self.construct_puzzle(ctx)?;
        let puzzle_reveal = ctx.serialize(&puzzle_reveal)?;

        let solution = ctx.serialize(&SlotSolution {
            lineage_proof: self.proof.into(),
            spender_inner_puzzle_hash,
        })?;

        ctx.insert(CoinSpend::new(self.coin, puzzle_reveal, solution));

        Ok(())
    }
}
