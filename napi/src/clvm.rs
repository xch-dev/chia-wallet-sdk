use chia::{
    clvm_traits::ClvmEncoder,
    clvm_utils::{self, CurriedProgram, TreeHash},
    protocol::Bytes32,
};
use chia_wallet_sdk::SpendContext;
use clvmr::{
    run_program,
    serde::{node_from_bytes, node_from_bytes_backrefs},
    ChiaDialect, NodePtr, ENABLE_BLS_OPS_OUTSIDE_GUARD, ENABLE_FIXED_DIV, MEMPOOL_MODE,
};
use napi::bindgen_prelude::*;

use crate::{
    delegated_spend_for_conditions, mint_nfts, parse_nft_info, parse_unspent_nft, spend_nft,
    spend_p2_delegated_singleton, spend_p2_standard,
    traits::{IntoJs, IntoRust},
    Coin, CoinSpend, MintedNfts, Nft, NftMint, ParsedNft, Program, Spend,
};

type Clvm = Reference<ClvmAllocator>;

#[napi]
pub struct ClvmAllocator(pub(crate) SpendContext);

#[napi]
impl ClvmAllocator {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Ok(Self(SpendContext::new()))
    }

    #[napi(ts_args_type = "")]
    pub fn nil(&mut self, this: This<Clvm>) -> Result<Program> {
        Ok(Program {
            ctx: this,
            ptr: NodePtr::NIL,
        })
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize(&mut self, this: This<Clvm>, value: Uint8Array) -> Result<Program> {
        let ptr = node_from_bytes(&mut self.0.allocator, &value)?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize_with_backrefs(
        &mut self,
        this: This<Clvm>,
        value: Uint8Array,
    ) -> Result<Program> {
        let ptr = node_from_bytes_backrefs(&mut self.0.allocator, &value)?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi]
    pub fn tree_hash(&self, program: &Program) -> Result<Uint8Array> {
        self.0.tree_hash(program.ptr).to_bytes().into_js()
    }

    #[napi(
        ts_args_type = "puzzle: Program, solution: Program, maxCost: bigint, mempoolMode: boolean"
    )]
    pub fn run(
        &mut self,
        env: Env,
        this: This<Clvm>,
        puzzle: &Program,
        solution: &Program,
        max_cost: BigInt,
        mempool_mode: bool,
    ) -> Result<Output> {
        let mut flags = ENABLE_BLS_OPS_OUTSIDE_GUARD | ENABLE_FIXED_DIV;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        let result = run_program(
            &mut self.0.allocator,
            &ChiaDialect::new(flags),
            puzzle.ptr,
            solution.ptr,
            max_cost.into_rust()?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Output {
            value: Program {
                ctx: this,
                ptr: result.1,
            }
            .into_instance(env)?,
            cost: result.0.into_js()?,
        })
    }

    #[napi(ts_args_type = "program: Program, args: Array<Program>")]
    pub fn curry(
        &mut self,
        this: This<Clvm>,
        program: &Program,
        args: Vec<ClassInstance<Program>>,
    ) -> Result<Program> {
        let mut args_ptr = self.0.allocator.one();

        for arg in args.into_iter().rev() {
            args_ptr = self
                .0
                .allocator
                .encode_curried_arg(arg.ptr, args_ptr)
                .map_err(|error| Error::from_reason(error.to_string()))?;
        }

        self.0
            .alloc(&CurriedProgram {
                program: program.ptr,
                args: args_ptr,
            })
            .map_err(|error| Error::from_reason(error.to_string()))
            .map(|ptr| Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "values: Array<Program>")]
    pub fn new_list(
        &mut self,
        this: This<Clvm>,
        values: Vec<ClassInstance<Program>>,
    ) -> Result<Program> {
        let items: Vec<NodePtr> = values.into_iter().map(|program| program.ptr).collect();
        let ptr = self
            .0
            .alloc(&items)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "first: Program, rest: Program")]
    pub fn new_pair(
        &mut self,
        this: This<Clvm>,
        first: &Program,
        rest: &Program,
    ) -> Result<Program> {
        let ptr = self
            .0
            .allocator
            .new_pair(first.ptr, rest.ptr)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn new_atom(&mut self, this: This<Clvm>, value: Uint8Array) -> Result<Program> {
        let value: Vec<u8> = value.into_rust()?;
        let ptr = self
            .0
            .allocator
            .new_atom(&value)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: string")]
    pub fn new_string(&mut self, this: This<Clvm>, value: String) -> Result<Program> {
        let ptr = self
            .0
            .allocator
            .new_atom(value.as_bytes())
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: number")]
    pub fn new_small_number(&mut self, this: This<Clvm>, value: u32) -> Result<Program> {
        // TODO: Upstream a better check to clvmr?
        if value > 67_108_863 {
            return Err(Error::from_reason(
                "Value is too large to fit in a small number".to_string(),
            ));
        }

        let ptr = self
            .0
            .allocator
            .new_small_number(value)
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: bigint")]
    pub fn new_big_int(&mut self, this: This<Clvm>, value: BigInt) -> Result<Program> {
        let value = value.into_rust()?;
        let ptr = self
            .0
            .allocator
            .new_number(value)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "conditions: Array<Program>")]
    pub fn delegated_spend_for_conditions(
        &mut self,
        env: Env,
        this: This<Clvm>,
        conditions: Vec<ClassInstance<Program>>,
    ) -> Result<Spend> {
        delegated_spend_for_conditions(env, this, conditions)
    }

    #[napi(ts_args_type = "syntheticKey: Uint8Array, delegatedSpend: Spend")]
    pub fn spend_p2_standard(
        &mut self,
        env: Env,
        this: This<Clvm>,
        synthetic_key: Uint8Array,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        spend_p2_standard(env, this, synthetic_key, delegated_spend)
    }

    #[napi(
        ts_args_type = "launcherId: Uint8Array, coinId: Uint8Array, singletonInnerPuzzleHash: Uint8Array, delegatedSpend: Spend"
    )]
    pub fn spend_p2_delegated_singleton(
        &mut self,
        env: Env,
        this: This<Clvm>,
        launcher_id: Uint8Array,
        coin_id: Uint8Array,
        singleton_inner_puzzle_hash: Uint8Array,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        spend_p2_delegated_singleton(
            env,
            this,
            launcher_id,
            coin_id,
            singleton_inner_puzzle_hash,
            delegated_spend,
        )
    }

    #[napi(ts_args_type = "parent_coin_id: Uint8Array, nft_mints: Array<NftMint>")]
    pub fn mint_nfts(
        &mut self,
        env: Env,
        this: This<Clvm>,
        parent_coin_id: Uint8Array,
        nft_mints: Vec<NftMint>,
    ) -> Result<MintedNfts> {
        mint_nfts(env, this, parent_coin_id, nft_mints)
    }

    #[napi(ts_args_type = "puzzle: Program")]
    pub fn parse_nft_info(
        &mut self,
        env: Env,
        this: This<Clvm>,
        puzzle: &Program,
    ) -> Result<Option<ParsedNft>> {
        parse_nft_info(env, this, puzzle)
    }

    #[napi]
    pub fn parse_unspent_nft(
        &mut self,
        parent_coin: Coin,
        parent_puzzle: &Program,
        parent_solution: &Program,
        coin: Coin,
    ) -> Result<Option<Nft>> {
        parse_unspent_nft(self, parent_coin, parent_puzzle, parent_solution, coin)
    }

    #[napi]
    pub fn spend_nft(&mut self, nft: Nft, inner_spend: Spend) -> Result<Vec<CoinSpend>> {
        spend_nft(self, nft, inner_spend)
    }
}

#[napi(object)]
pub struct Output {
    pub value: ClassInstance<Program>,
    pub cost: BigInt,
}

#[napi]
pub fn curry_tree_hash(tree_hash: Uint8Array, args: Vec<Uint8Array>) -> Result<Uint8Array> {
    let tree_hash: Bytes32 = tree_hash.into_rust()?;
    let args: Vec<TreeHash> = args
        .into_iter()
        .map(|arg| Ok(TreeHash::new(arg.into_rust()?)))
        .collect::<Result<Vec<_>>>()?;
    clvm_utils::curry_tree_hash(tree_hash.into(), &args)
        .to_bytes()
        .into_js()
}
