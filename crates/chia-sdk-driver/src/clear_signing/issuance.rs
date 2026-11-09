use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Condition, Mod, puzzles::EverythingWithSingletonTailArgs};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{CurriedPuzzle, DriverError};

/// A CAT TAIL run by a verified spend, which issues or melts some of the CAT.
#[derive(Debug, Clone)]
pub struct Issuance {
    /// The id of the CAT coin whose spend runs the TAIL.
    pub coin_id: Bytes32,
    /// The tree hash of the TAIL program. Only issuances whose asset id matches the spent CAT affect the
    /// [`AssetFlow`](crate::AssetFlow)s.
    pub asset_id: Bytes32,
    /// The extra delta of the CAT ring. A positive value issues that much of the CAT, and a negative value melts
    /// it into XCH.
    ///
    /// This comes from the spend's solution, so it can only be trusted if [`verified`](Self::verified) is true.
    pub extra_delta: i64,
    /// The kind of TAIL, if it's recognized.
    pub kind: IssuanceKind,
    /// Whether the extra delta was authorized by a message from the vault. If not, the extra delta
    /// may be different when the transaction is included in a block, so it should not be relied on.
    pub verified: bool,
}

/// The kind of TAIL run by an [`Issuance`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssuanceKind {
    /// An everything with singleton TAIL, which requires the singleton to send a message that commits to the
    /// extra delta.
    EverythingWithSingleton {
        /// The hash of the singleton struct of the singleton that controls issuance.
        singleton_struct_hash: Bytes32,
        /// The nonce, which allows one singleton to control more than one CAT.
        nonce: Bytes,
    },
    /// Any other TAIL.
    Unknown,
}

/// A `RUN_CAT_TAIL` condition, as parsed by [`parse_run_cat_tail`].
#[derive(Debug, Clone)]
pub struct RunCatTailInvocation {
    /// The tree hash of the TAIL program.
    pub asset_id: Bytes32,
    /// The kind of TAIL.
    pub kind: IssuanceKind,
}

/// Parses the first `RUN_CAT_TAIL` condition in the list, if there is one.
pub fn parse_run_cat_tail(
    allocator: &Allocator,
    conditions: &[Condition],
) -> Result<Option<RunCatTailInvocation>, DriverError> {
    for condition in conditions {
        let Some(run_cat_tail) = condition.as_run_cat_tail() else {
            continue;
        };

        let asset_id = tree_hash(allocator, run_cat_tail.program).into();
        let kind = classify_tail(allocator, run_cat_tail.program)?;

        return Ok(Some(RunCatTailInvocation { asset_id, kind }));
    }

    Ok(None)
}

fn classify_tail(allocator: &Allocator, tail: NodePtr) -> Result<IssuanceKind, DriverError> {
    let Some(curried) = CurriedPuzzle::parse(allocator, tail) else {
        return Ok(IssuanceKind::Unknown);
    };

    if curried.mod_hash != EverythingWithSingletonTailArgs::mod_hash() {
        return Ok(IssuanceKind::Unknown);
    }

    let args = EverythingWithSingletonTailArgs::from_clvm(allocator, curried.args)?;

    Ok(IssuanceKind::EverythingWithSingleton {
        singleton_struct_hash: args.singleton_struct_hash,
        nonce: args.nonce,
    })
}

/// The message that a vault sends to authorize a CAT issuance with an everything with singleton TAIL, which is the
/// CLVM atom encoding of the extra delta.
pub fn get_extra_delta_message(extra_delta: i64) -> Bytes {
    let mut allocator = Allocator::new();
    let ptr = extra_delta.to_clvm(&mut allocator).unwrap();
    allocator.atom(ptr).as_ref().to_vec().into()
}
