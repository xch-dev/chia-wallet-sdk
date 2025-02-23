mod curried_program;
mod output;
mod pair;
mod program;
mod puzzle;
mod spend;
mod value;

use chia_sdk_bindings::Bytes32;
pub use curried_program::*;
pub use output::*;
pub use pair::*;
pub use program::*;
pub use puzzle::*;
pub use spend::*;

use clvmr::NodePtr;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use value::{alloc, spend_to_js, Value};

pub(crate) use value::clvm;

use crate::{
    CatSpend, Coin, CoinSpend, IntoJs, IntoJsWithClvm, IntoRust, MintedNfts, NftMint, PublicKey,
};

#[napi]
#[derive(Default)]
pub struct Clvm(pub(crate) chia_sdk_bindings::Clvm);

#[napi]
impl Clvm {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi]
    pub fn alloc<'a>(&mut self, env: Env, this: This<'a>, value: Value<'a>) -> Result<Program> {
        let clvm = clvm(env, this)?;
        let node_ptr = alloc(env, clvm.clone(env)?, value)?;
        Ok(Program { clvm, node_ptr })
    }

    #[napi]
    pub fn nil(&mut self, env: Env, this: This<'_>) -> Result<Program> {
        let clvm = clvm(env, this)?;
        Ok(Program {
            clvm,
            node_ptr: NodePtr::NIL,
        })
    }

    #[napi]
    pub fn pair<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        first: Value<'a>,
        rest: Value<'a>,
    ) -> Result<Program> {
        let mut clvm = clvm(env, this)?;
        let first = alloc(env, clvm.clone(env)?, first)?;
        let rest = alloc(env, clvm.clone(env)?, rest)?;
        let node_ptr = clvm.0.new_pair(first, rest)?;
        Ok(Program { clvm, node_ptr })
    }

    #[napi]
    pub fn deserialize(&mut self, env: Env, this: This<'_>, value: Uint8Array) -> Result<Program> {
        let clvm = clvm(env, this)?;
        let node_ptr = self.0.deserialize(value.rust()?)?;
        Ok(Program { clvm, node_ptr })
    }

    #[napi]
    pub fn deserialize_with_backrefs(
        &mut self,
        env: Env,
        this: This<'_>,
        value: Uint8Array,
    ) -> Result<Program> {
        let clvm = clvm(env, this)?;
        let node_ptr = self.0.deserialize_with_backrefs(value.rust()?)?;
        Ok(Program { clvm, node_ptr })
    }

    #[napi]
    pub fn insert_coin_spend(&mut self, coin_spend: CoinSpend) -> Result<()> {
        self.0.insert_coin_spend(coin_spend.rust()?);
        Ok(())
    }

    #[napi]
    pub fn coin_spends(&mut self) -> Result<Vec<CoinSpend>> {
        Ok(self
            .0
            .take_coin_spends()
            .into_iter()
            .map(IntoJs::js)
            .collect::<chia_sdk_bindings::Result<Vec<_>>>()?)
    }

    #[napi]
    pub fn spend_coin(&mut self, coin: Coin, spend: Spend) -> Result<()> {
        let puzzle_reveal = self.0.serialize(spend.puzzle.node_ptr)?;
        let solution = self.0.serialize(spend.solution.node_ptr)?;
        self.0.insert_coin_spend(chia_sdk_bindings::CoinSpend::new(
            coin.rust()?,
            puzzle_reveal,
            solution,
        ));
        Ok(())
    }

    #[napi]
    pub fn delegated_spend(
        &mut self,
        env: Env,
        this: This<'_>,
        conditions: Vec<ClassInstance<'_, Program>>,
    ) -> Result<Spend> {
        let clvm = clvm(env, this)?;

        let conditions: Vec<NodePtr> = conditions
            .into_iter()
            .map(|program| program.node_ptr)
            .collect();

        let spend = self.0.delegated_spend(conditions)?;

        spend_to_js(env, clvm, spend)
    }

    #[napi]
    pub fn standard_spend(
        &mut self,
        env: Env,
        this: This<'_>,
        synthetic_key: &PublicKey,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        let clvm = clvm(env, this)?;

        let spend = self.0.standard_spend(
            synthetic_key.0,
            chia_sdk_bindings::Spend {
                puzzle: delegated_spend.puzzle.node_ptr,
                solution: delegated_spend.solution.node_ptr,
            },
        )?;

        spend_to_js(env, clvm, spend)
    }

    #[napi]
    pub fn spend_standard_coin(
        &mut self,
        coin: Coin,
        synthetic_key: &PublicKey,
        delegated_spend: Spend,
    ) -> Result<()> {
        self.0.spend_standard_coin(
            coin.rust()?,
            synthetic_key.0,
            chia_sdk_bindings::Spend {
                puzzle: delegated_spend.puzzle.node_ptr,
                solution: delegated_spend.solution.node_ptr,
            },
        )?;
        Ok(())
    }

    #[napi]
    pub fn spend_cat_coins(&mut self, cats: Vec<CatSpend>) -> Result<()> {
        self.0.spend_cat_coins(
            cats.into_iter()
                .map(|item| {
                    Ok(chia_sdk_bindings::CatSpend::new(
                        item.cat.rust()?,
                        chia_sdk_bindings::Spend::new(
                            item.spend.puzzle.node_ptr,
                            item.spend.solution.node_ptr,
                        ),
                    ))
                })
                .collect::<chia_sdk_bindings::Result<Vec<_>>>()?,
        )?;
        Ok(())
    }

    pub fn mint_nfts(
        &mut self,
        env: Env,
        this: This<'_>,
        parent_coin_id: Uint8Array,
        nft_mints: Vec<NftMint>,
    ) -> Result<MintedNfts> {
        let clvm = clvm(env, this)?;

        let (nfts, parent_conditions) = self.0.mint_nfts(
            parent_coin_id.rust()?,
            nft_mints
                .into_iter()
                .map(|nft| nft.rust())
                .collect::<chia_sdk_bindings::Result<Vec<_>>>()?,
        )?;

        Ok(MintedNfts {
            nfts: nfts
                .into_iter()
                .map(|nft| nft.js_with_clvm(env, &clvm)?)
                .collect(),
            parent_conditions: parent_conditions
                .into_iter()
                .map(|condition| condition.js_with_clvm(env, &clvm)?)
                .collect(),
        })
    }
}
