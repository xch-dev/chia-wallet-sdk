use chia::{
    bls::PublicKey,
    clvm_traits::{clvm_quote, ToClvm},
};
use chia_wallet_sdk::{self as sdk, P2DelegatedSingletonLayer, StandardLayer};
use clvmr::NodePtr;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoRust},
    ClvmAllocator, ClvmPtr,
};

#[napi(object)]
pub struct Spend {
    pub puzzle: ClassInstance<ClvmPtr>,
    pub solution: ClassInstance<ClvmPtr>,
}

#[napi]
pub fn delegated_spend_for_conditions(
    env: Env,
    clvm: &mut ClvmAllocator,
    conditions: Vec<ClassInstance<ClvmPtr>>,
) -> Result<Spend> {
    let conditions: Vec<NodePtr> = conditions.into_iter().map(|ptr| ptr.0).collect();

    let delegated_puzzle = clvm_quote!(conditions)
        .to_clvm(&mut clvm.0)
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: ClvmPtr(delegated_puzzle).into_instance(env)?,
        solution: ClvmPtr(NodePtr::NIL).into_instance(env)?,
    })
}

#[napi]
pub fn spend_p2_standard(
    env: Env,
    clvm: &mut ClvmAllocator,
    synthetic_key: Uint8Array,
    delegated_spend: Spend,
) -> Result<Spend> {
    clvm.with_context(|ctx| {
        let synthetic_key = PublicKey::from_js(synthetic_key)?;
        let p2 = StandardLayer::new(synthetic_key);

        let spend = p2
            .delegated_inner_spend(
                ctx,
                sdk::Spend {
                    puzzle: delegated_spend.puzzle.0,
                    solution: delegated_spend.solution.0,
                },
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: ClvmPtr(spend.puzzle).into_instance(env)?,
            solution: ClvmPtr(spend.solution).into_instance(env)?,
        })
    })
}

#[napi]
pub fn spend_p2_delegated_singleton(
    env: Env,
    clvm: &mut ClvmAllocator,
    launcher_id: Uint8Array,
    coin_id: Uint8Array,
    singleton_inner_puzzle_hash: Uint8Array,
    delegated_spend: Spend,
) -> Result<Spend> {
    clvm.with_context(|ctx| {
        let p2 = P2DelegatedSingletonLayer::new(launcher_id.into_rust()?);

        let spend = p2
            .spend(
                ctx,
                coin_id.into_rust()?,
                singleton_inner_puzzle_hash.into_rust()?,
                sdk::Spend {
                    puzzle: delegated_spend.puzzle.0,
                    solution: delegated_spend.solution.0,
                },
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: ClvmPtr(spend.puzzle).into_instance(env)?,
            solution: ClvmPtr(spend.solution).into_instance(env)?,
        })
    })
}
