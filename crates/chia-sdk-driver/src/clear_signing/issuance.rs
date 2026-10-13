use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Condition, Mod, puzzles::EverythingWithSingletonTailArgs};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{CurriedPuzzle, DriverError};

#[derive(Debug, Clone)]
pub struct Issuance {
    pub coin_id: Bytes32,
    pub asset_id: Bytes32,
    /// This comes from the spend's solution, so it can only be trusted if `verified` is true.
    pub extra_delta: i64,
    pub kind: IssuanceKind,
    /// Whether the extra delta was authorized by a message from the vault. If not, the extra delta
    /// may be different when the transaction is included in a block, so it should not be relied on.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssuanceKind {
    EverythingWithSingleton {
        singleton_struct_hash: Bytes32,
        nonce: Bytes,
    },
    Unknown,
}

#[derive(Debug, Clone)]
pub struct RunCatTailInvocation {
    pub asset_id: Bytes32,
    pub kind: IssuanceKind,
}

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

pub fn get_extra_delta_message(extra_delta: i64) -> Bytes {
    let mut allocator = Allocator::new();
    let ptr = extra_delta.to_clvm(&mut allocator).unwrap();
    allocator.atom(ptr).as_ref().to_vec().into()
}
