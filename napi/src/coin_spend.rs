use chia::protocol;
use chia_wallet_sdk as sdk;
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoJs, IntoJsContextual, IntoRust},
    ClvmAllocator, Coin, Program,
};

#[napi(object)]
pub struct CoinSpend {
    pub coin: Coin,
    pub puzzle_reveal: Uint8Array,
    pub solution: Uint8Array,
}

impl IntoJs<CoinSpend> for protocol::CoinSpend {
    fn into_js(self) -> Result<CoinSpend> {
        Ok(CoinSpend {
            coin: self.coin.into_js()?,
            puzzle_reveal: self.puzzle_reveal.into_js()?,
            solution: self.solution.into_js()?,
        })
    }
}

impl FromJs<CoinSpend> for protocol::CoinSpend {
    fn from_js(coin_spend: CoinSpend) -> Result<Self> {
        Ok(protocol::CoinSpend {
            coin: coin_spend.coin.into_rust()?,
            puzzle_reveal: coin_spend.puzzle_reveal.into_rust()?,
            solution: coin_spend.solution.into_rust()?,
        })
    }
}

#[napi(object)]
pub struct Spend {
    pub puzzle: ClassInstance<Program>,
    pub solution: ClassInstance<Program>,
}

impl IntoJsContextual<Spend> for sdk::Spend {
    fn into_js_contextual(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
        clvm_allocator: &mut ClvmAllocator,
    ) -> Result<Spend> {
        Ok(Spend {
            puzzle: self
                .puzzle
                .into_js_contextual(env, this.clone(env)?, clvm_allocator)?,
            solution: self
                .solution
                .into_js_contextual(env, this, clvm_allocator)?,
        })
    }
}

impl FromJs<Spend> for sdk::Spend {
    fn from_js(spend: Spend) -> Result<Self> {
        Ok(sdk::Spend {
            puzzle: spend.puzzle.into_rust()?,
            solution: spend.solution.into_rust()?,
        })
    }
}
