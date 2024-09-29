use chia::{bls::PublicKey, clvm_traits::clvm_quote};
use chia_wallet_sdk as sdk;
use clvmr::NodePtr;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoRust},
    ClvmAllocator, Program,
};

#[napi(object)]
pub struct Spend {
    pub puzzle: ClassInstance<Program>,
    pub solution: ClassInstance<Program>,
}

pub fn delegated_spend_for_conditions(
    env: Env,
    mut clvm: Reference<ClvmAllocator>,
    conditions: Vec<ClassInstance<Program>>,
) -> Result<Spend> {
    let conditions: Vec<NodePtr> = conditions.into_iter().map(|program| program.ptr).collect();

    let delegated_puzzle = clvm
        .0
        .alloc(&clvm_quote!(conditions))
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: Program {
            ctx: clvm.clone(env)?,
            ptr: delegated_puzzle,
        }
        .into_instance(env)?,
        solution: Program {
            ctx: clvm,
            ptr: NodePtr::NIL,
        }
        .into_instance(env)?,
    })
}

pub fn spend_p2_standard(
    env: Env,
    mut clvm: Reference<ClvmAllocator>,
    synthetic_key: Uint8Array,
    delegated_spend: Spend,
) -> Result<Spend> {
    let ctx = &mut clvm.0;
    let synthetic_key = PublicKey::from_js(synthetic_key)?;
    let p2 = sdk::StandardLayer::new(synthetic_key);

    let spend = p2
        .delegated_inner_spend(
            ctx,
            sdk::Spend {
                puzzle: delegated_spend.puzzle.ptr,
                solution: delegated_spend.solution.ptr,
            },
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: Program {
            ctx: clvm.clone(env)?,
            ptr: spend.puzzle,
        }
        .into_instance(env)?,
        solution: Program {
            ctx: clvm,
            ptr: spend.solution,
        }
        .into_instance(env)?,
    })
}

pub fn spend_p2_delegated_singleton(
    env: Env,
    mut clvm: Reference<ClvmAllocator>,
    launcher_id: Uint8Array,
    coin_id: Uint8Array,
    singleton_inner_puzzle_hash: Uint8Array,
    delegated_spend: Spend,
) -> Result<Spend> {
    let p2 = sdk::P2DelegatedSingletonLayer::new(launcher_id.into_rust()?);

    let spend = p2
        .spend(
            &mut clvm.0,
            coin_id.into_rust()?,
            singleton_inner_puzzle_hash.into_rust()?,
            sdk::Spend {
                puzzle: delegated_spend.puzzle.ptr,
                solution: delegated_spend.solution.ptr,
            },
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: Program {
            ctx: clvm.clone(env)?,
            ptr: spend.puzzle,
        }
        .into_instance(env)?,
        solution: Program {
            ctx: clvm,
            ptr: spend.solution,
        }
        .into_instance(env)?,
    })
}
