use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_sdk_types::{Condition, Mod, puzzles::EverythingWithSingletonTailArgs};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{CurriedPuzzle, DriverError};

#[derive(Debug, Clone, Copy)]
pub struct Issuance {
    pub coin_id: Bytes32,
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
    pub extra_delta: i64,
    pub kind: IssuanceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuanceKind {
    Singleton {
        singleton_struct_hash: Bytes32,
        nonce: usize,
    },

    Other,
}

impl IssuanceKind {
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

#[derive(Debug, Clone, Copy)]
pub struct RunCatTailInvocation {
    pub asset_id: Bytes32,
    pub kind: IssuanceKind,
}

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
