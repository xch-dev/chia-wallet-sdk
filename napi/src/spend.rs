use chia::{bls::PublicKey, clvm_traits::FromClvm};
use chia_wallet_sdk::{
    Condition, Conditions, P2Singleton, SpendContext, SpendWithConditions, StandardLayer,
};
use clvmr::{
    serde::{node_from_bytes, node_to_bytes},
    NodePtr,
};
use napi::bindgen_prelude::*;

use crate::traits::{FromJs, IntoRust};

#[napi(object)]
pub struct Spend {
    pub puzzle: Uint8Array,
    pub solution: Uint8Array,
}

#[napi]
pub fn spend_p2_standard(synthetic_key: Uint8Array, conditions: Vec<Uint8Array>) -> Result<Spend> {
    let mut ctx = SpendContext::new();

    let synthetic_key = PublicKey::from_js(synthetic_key)?;
    let p2 = StandardLayer::new(synthetic_key);

    let mut spend_conditions = Conditions::new();

    for condition in conditions {
        let condition = node_from_bytes(&mut ctx.allocator, &condition)?;
        spend_conditions = spend_conditions.with(
            Condition::<NodePtr>::from_clvm(&ctx.allocator, condition)
                .map_err(|error| Error::from_reason(error.to_string()))?,
        );
    }

    let spend = p2
        .spend_with_conditions(&mut ctx, spend_conditions)
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: node_to_bytes(&ctx.allocator, spend.puzzle)?.into(),
        solution: node_to_bytes(&ctx.allocator, spend.solution)?.into(),
    })
}

#[napi]
pub fn spend_p2_singleton(
    launcher_id: Uint8Array,
    coin_id: Uint8Array,
    singleton_inner_puzzle_hash: Uint8Array,
) -> Result<Spend> {
    let mut ctx = SpendContext::new();

    let p2 = P2Singleton::new(launcher_id.into_rust()?);

    let spend = p2
        .spend(
            &mut ctx,
            coin_id.into_rust()?,
            singleton_inner_puzzle_hash.into_rust()?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

    Ok(Spend {
        puzzle: node_to_bytes(&ctx.allocator, spend.puzzle)?.into(),
        solution: node_to_bytes(&ctx.allocator, spend.solution)?.into(),
    })
}
