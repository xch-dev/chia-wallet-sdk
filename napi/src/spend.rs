use chia::{bls::PublicKey, clvm_traits::FromClvm};
use chia_wallet_sdk::{Condition, Conditions, P2Singleton, SpendWithConditions, StandardLayer};
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
pub fn spend_p2_standard(
    env: Env,
    clvm: &mut ClvmAllocator,
    synthetic_key: Uint8Array,
    conditions: Vec<ClassInstance<ClvmPtr>>,
) -> Result<Spend> {
    clvm.with_context(|ctx| {
        let synthetic_key = PublicKey::from_js(synthetic_key)?;
        let p2 = StandardLayer::new(synthetic_key);

        let mut spend_conditions = Conditions::new();

        for condition in conditions {
            spend_conditions = spend_conditions.with(
                Condition::<NodePtr>::from_clvm(&ctx.allocator, condition.0)
                    .map_err(|error| Error::from_reason(error.to_string()))?,
            );
        }

        let spend = p2
            .spend_with_conditions(ctx, spend_conditions)
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: ClvmPtr(spend.puzzle).into_instance(env)?,
            solution: ClvmPtr(spend.solution).into_instance(env)?,
        })
    })
}

#[napi]
pub fn spend_p2_singleton(
    env: Env,
    clvm: &mut ClvmAllocator,
    launcher_id: Uint8Array,
    coin_id: Uint8Array,
    singleton_inner_puzzle_hash: Uint8Array,
) -> Result<Spend> {
    clvm.with_context(|ctx| {
        let p2 = P2Singleton::new(launcher_id.into_rust()?);

        let spend = p2
            .spend(
                ctx,
                coin_id.into_rust()?,
                singleton_inner_puzzle_hash.into_rust()?,
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: ClvmPtr(spend.puzzle).into_instance(env)?,
            solution: ClvmPtr(spend.solution).into_instance(env)?,
        })
    })
}
