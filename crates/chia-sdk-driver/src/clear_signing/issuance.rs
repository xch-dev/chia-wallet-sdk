use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatSolution, singleton::SingletonStruct};
use chia_sdk_types::{Condition, Mod, puzzles::EverythingWithSingletonTailArgs};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{CurriedPuzzle, DriverError};

/// A CAT supply change authorized by a vault spend.
///
/// An issuance is recorded whenever a coin spend that is fully verified through custody
/// (a P2 singleton, P2 conditions or singleton, or top-level delegated conditions) outputs a
/// `RunCatTail` condition. Because the conditions of those spends are pinned by the vault's
/// signature, the issuance can be trusted to happen as described.
#[derive(Debug, Clone, Copy)]
pub struct Issuance {
    /// The coin id of the spend that includes the `RunCatTail` condition.
    /// This is always also the coin id of one of the `VerifiedSpend`s in the transaction.
    pub coin_id: Bytes32,
    /// The asset id (= tree hash of the TAIL puzzle) of the CAT being issued/melted.
    pub asset_id: Bytes32,
    /// If the CAT is revocable, the hidden puzzle hash on the issuing coin.
    pub hidden_puzzle_hash: Option<Bytes32>,
    /// The supply change as signed by the TAIL — the `extra_delta` from the CAT layer's solution
    /// for the spend that ran the TAIL. Negative values mean the supply is being increased (more
    /// output than input across the ring); positive values mean it is being decreased (melting).
    pub delta: i64,
    /// What kind of TAIL was used to authorize this issuance.
    pub kind: IssuanceKind,
}

/// The kind of TAIL puzzle authorizing an [`Issuance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuanceKind {
    /// The TAIL is the standard `EverythingWithSingleton` TAIL, allowing a singleton with the given
    /// `singleton_struct_hash` to authorize issuances by sending it a message containing the delta.
    Singleton {
        /// The hash of `(SINGLETON_MOD_HASH . (LAUNCHER_ID . SINGLETON_LAUNCHER_HASH))`.
        ///
        /// Compare against `SingletonStruct::new(my_launcher_id).tree_hash()` to determine
        /// whether the issuance is authorized by the user's vault or by some other singleton.
        singleton_struct_hash: Bytes32,
        /// The nonce in the curried TAIL args. Different nonces are different asset ids
        /// even for the same singleton.
        nonce: usize,
    },
    /// The TAIL is something else: a single-issuance TAIL, multi-issuance via signature, or any
    /// custom puzzle. The user is authorizing this issuance through inner-puzzle custody alone.
    Other,
}

impl IssuanceKind {
    /// Whether this issuance is authorized by the singleton with the given launcher id.
    /// Only meaningful for `IssuanceKind::Singleton`; always returns false otherwise.
    pub fn is_singleton(&self, launcher_id: Bytes32) -> bool {
        match self {
            Self::Singleton {
                singleton_struct_hash,
                ..
            } => *singleton_struct_hash == SingletonStruct::new(launcher_id).tree_hash().into(),
            Self::Other => false,
        }
    }
}

/// A `RunCatTail` invocation that has been parsed out of trusted inner conditions.
/// This is the intermediate form used by [`parse_vault_transaction`](crate::parse_vault_transaction)
/// to match vault `SendMessage`s against TAIL `RECEIVE_MESSAGE`s.
#[derive(Debug, Clone, Copy)]
pub struct RunCatTailInvocation {
    pub asset_id: Bytes32,
    pub kind: IssuanceKind,
}

/// Find every `RunCatTail` condition in a list of trusted inner conditions, classifying each by
/// the curried TAIL puzzle. The conditions must already have been pinned by the vault (i.e. taken
/// from a custody-authorized spend), otherwise the issuances they describe cannot be trusted.
pub fn parse_run_cat_tails(
    allocator: &Allocator,
    conditions: &[Condition],
) -> Result<Vec<RunCatTailInvocation>, DriverError> {
    let mut invocations = Vec::new();

    for condition in conditions {
        let Some(run_cat_tail) = condition.as_run_cat_tail() else {
            continue;
        };

        let asset_id = tree_hash(allocator, run_cat_tail.program).into();
        let kind = classify_tail(allocator, run_cat_tail.program)?;

        invocations.push(RunCatTailInvocation { asset_id, kind });
    }

    Ok(invocations)
}

/// Try to recognize a TAIL puzzle as `EverythingWithSingleton`, falling back to `IssuanceKind::Other`.
fn classify_tail(allocator: &Allocator, tail: NodePtr) -> Result<IssuanceKind, DriverError> {
    let Some(curried) = CurriedPuzzle::parse(allocator, tail) else {
        return Ok(IssuanceKind::Other);
    };

    if curried.mod_hash != EverythingWithSingletonTailArgs::mod_hash() {
        return Ok(IssuanceKind::Other);
    }

    let args = EverythingWithSingletonTailArgs::from_clvm(allocator, curried.args)?;

    Ok(IssuanceKind::Singleton {
        singleton_struct_hash: args.singleton_struct_hash,
        nonce: args.nonce,
    })
}

/// Read the `extra_delta` field of a CAT spend's outer solution.
///
/// `extra_delta` is the value the CAT layer hands to the TAIL puzzle. It is computed by the
/// builder of the spend (not the CAT layer itself) so that the ring balances:
///
/// ```text
/// extra_delta = -sum_over_ring(coin.amount - sum(inner CreateCoin amounts))
/// ```
///
/// We can't faithfully reproduce that here from a single coin's conditions, but we don't have to:
/// the value is committed to in this coin's solution, and the CAT layer enforces that it matches
/// the ring math at validation time. So reading it back from the solution is exactly the value
/// the TAIL receives, regardless of how many coins are in the ring.
pub fn parse_cat_extra_delta(
    allocator: &Allocator,
    outer_cat_solution: NodePtr,
) -> Result<i64, DriverError> {
    let solution = CatSolution::<NodePtr>::from_clvm(allocator, outer_cat_solution)?;
    Ok(solution.extra_delta)
}
