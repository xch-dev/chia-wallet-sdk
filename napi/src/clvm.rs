use std::mem;

use chia::{
    clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, ToClvm},
    clvm_utils::{tree_hash, CurriedProgram},
};
use chia_wallet_sdk::SpendContext;
use clvmr::{
    run_program,
    serde::{node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs},
    ChiaDialect, NodePtr, SExp, ENABLE_BLS_OPS_OUTSIDE_GUARD, ENABLE_FIXED_DIV, MEMPOOL_MODE,
};
use napi::bindgen_prelude::*;

use crate::traits::{IntoJs, IntoRust};

#[napi]
pub struct ClvmAllocator(pub(crate) clvmr::Allocator);

impl ClvmAllocator {
    pub fn with_context<T>(&mut self, f: impl FnOnce(&mut SpendContext) -> T) -> T {
        let mut ctx = SpendContext::from(mem::take(&mut self.0));
        let result = f(&mut ctx);
        self.0 = ctx.allocator;
        result
    }
}

#[napi]
impl ClvmAllocator {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Ok(Self(clvmr::Allocator::new()))
    }

    #[napi]
    pub fn deserialize(&mut self, value: Uint8Array) -> Result<ClvmPtr> {
        let ptr = node_from_bytes(&mut self.0, &value)?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn deserialize_with_backrefs(&mut self, value: Uint8Array) -> Result<ClvmPtr> {
        let ptr = node_from_bytes_backrefs(&mut self.0, &value)?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn serialize(&self, ptr: &ClvmPtr) -> Result<Uint8Array> {
        let bytes = node_to_bytes(&self.0, ptr.0)?;
        Ok(bytes.into_js().unwrap())
    }

    #[napi]
    pub fn serialize_with_backrefs(&self, ptr: &ClvmPtr) -> Result<Uint8Array> {
        let bytes = node_to_bytes_backrefs(&self.0, ptr.0)?;
        Ok(bytes.into_js().unwrap())
    }

    #[napi]
    pub fn tree_hash(&self, ptr: &ClvmPtr) -> Result<Uint8Array> {
        tree_hash(&self.0, ptr.0).to_bytes().into_js()
    }

    #[napi]
    pub fn run(
        &mut self,
        env: Env,
        puzzle: &ClvmPtr,
        solution: &ClvmPtr,
        max_cost: BigInt,
        mempool_mode: bool,
    ) -> Result<Output> {
        let mut flags = ENABLE_BLS_OPS_OUTSIDE_GUARD | ENABLE_FIXED_DIV;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        let result = run_program(
            &mut self.0,
            &ChiaDialect::new(flags),
            puzzle.0,
            solution.0,
            max_cost.into_rust()?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Output {
            value: ClvmPtr(result.1).into_instance(env)?,
            cost: result.0.into_js()?,
        })
    }

    #[napi]
    pub fn curry(&mut self, ptr: &ClvmPtr, args: Vec<ClassInstance<ClvmPtr>>) -> Result<ClvmPtr> {
        let mut args_ptr = self.0.one();

        for arg in args.into_iter().rev() {
            args_ptr = self
                .0
                .encode_curried_arg(arg.0, args_ptr)
                .map_err(|error| Error::from_reason(error.to_string()))?;
        }

        CurriedProgram {
            program: ptr.0,
            args: args_ptr,
        }
        .to_clvm(&mut self.0)
        .map_err(|error| Error::from_reason(error.to_string()))
        .map(ClvmPtr)
    }

    #[napi]
    pub fn uncurry(&self, env: Env, ptr: &ClvmPtr) -> Result<Option<Curry>> {
        let Ok(value) = CurriedProgram::<NodePtr, NodePtr>::from_clvm(&self.0, ptr.0) else {
            return Ok(None);
        };

        let mut args = Vec::new();
        let mut args_ptr = value.args;

        while let Ok((first, rest)) = self.0.decode_curried_arg(&args_ptr) {
            args.push(ClvmPtr(first).into_instance(env)?);
            args_ptr = rest;
        }

        if self.0.small_number(args_ptr) != Some(1) {
            return Ok(None);
        }

        Ok(Some(Curry {
            program: ClvmPtr(value.program).into_instance(env)?,
            args,
        }))
    }

    #[napi]
    pub fn new_list(&mut self, values: Vec<ClassInstance<ClvmPtr>>) -> Result<ClvmPtr> {
        let items: Vec<NodePtr> = values.into_iter().map(|ptr| ptr.0).collect();
        let ptr = items
            .to_clvm(&mut self.0)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn new_pair(&mut self, first: &ClvmPtr, rest: &ClvmPtr) -> Result<ClvmPtr> {
        let ptr = self
            .0
            .new_pair(first.0, rest.0)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn new_atom(&mut self, value: Uint8Array) -> Result<ClvmPtr> {
        let value: Vec<u8> = value.into_rust()?;
        let ptr = self
            .0
            .new_atom(&value)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn new_string(&mut self, value: String) -> Result<ClvmPtr> {
        let ptr = self
            .0
            .new_atom(value.as_bytes())
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn new_small_number(&mut self, value: u32) -> Result<ClvmPtr> {
        // TODO: Upstream a better check to clvmr?
        if value > 67_108_863 {
            return Err(Error::from_reason(
                "Value is too large to fit in a small number".to_string(),
            ));
        }

        let ptr = self
            .0
            .new_small_number(value)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn new_big_int(&mut self, value: BigInt) -> Result<ClvmPtr> {
        let value = value.into_rust()?;
        let ptr = self
            .0
            .new_number(value)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(ClvmPtr(ptr))
    }

    #[napi]
    pub fn list(&self, env: Env, ptr: &ClvmPtr) -> Result<Vec<ClassInstance<ClvmPtr>>> {
        let items = Vec::<NodePtr>::from_clvm(&self.0, ptr.0)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        items
            .into_iter()
            .map(|ptr| ClvmPtr(ptr).into_instance(env))
            .collect()
    }

    #[napi]
    pub fn pair(&self, env: Env, ptr: &ClvmPtr) -> Result<Option<Pair>> {
        let SExp::Pair(first, rest) = self.0.sexp(ptr.0) else {
            return Ok(None);
        };
        Ok(Some(Pair {
            first: ClvmPtr(first).into_instance(env)?,
            rest: ClvmPtr(rest).into_instance(env)?,
        }))
    }

    #[napi]
    pub fn atom(&self, ptr: &ClvmPtr) -> Option<Uint8Array> {
        match self.0.sexp(ptr.0) {
            SExp::Atom => Some(self.0.atom(ptr.0).as_ref().to_vec().into_js().unwrap()),
            SExp::Pair(..) => None,
        }
    }

    #[napi]
    pub fn atom_length(&self, ptr: &ClvmPtr) -> Option<u32> {
        match self.0.sexp(ptr.0) {
            SExp::Atom => self.0.atom_len(ptr.0).try_into().ok(),
            SExp::Pair(..) => None,
        }
    }

    #[napi]
    pub fn string(&self, ptr: &ClvmPtr) -> Option<String> {
        match self.0.sexp(ptr.0) {
            SExp::Atom => String::from_utf8(self.0.atom(ptr.0).as_ref().to_vec()).ok(),
            SExp::Pair(..) => None,
        }
    }

    #[napi]
    pub fn small_number(&self, ptr: &ClvmPtr) -> Option<u32> {
        match self.0.sexp(ptr.0) {
            SExp::Atom => self.0.small_number(ptr.0),
            SExp::Pair(..) => None,
        }
    }

    #[napi]
    pub fn big_int(&self, ptr: &ClvmPtr) -> Option<BigInt> {
        match self.0.sexp(ptr.0) {
            SExp::Atom => Some(self.0.number(ptr.0).into_js().unwrap()),
            SExp::Pair(..) => None,
        }
    }
}

#[napi]
pub struct ClvmPtr(pub(crate) clvmr::NodePtr);

#[napi]
impl ClvmPtr {
    #[napi(factory)]
    pub fn nil() -> Self {
        Self(NodePtr::NIL)
    }

    #[napi]
    pub fn is_atom(&self) -> bool {
        self.0.is_atom()
    }

    #[napi]
    pub fn is_pair(&self) -> bool {
        self.0.is_pair()
    }
}

#[napi(object)]
pub struct Pair {
    pub first: ClassInstance<ClvmPtr>,
    pub rest: ClassInstance<ClvmPtr>,
}

#[napi(object)]
pub struct Output {
    pub value: ClassInstance<ClvmPtr>,
    pub cost: BigInt,
}

#[napi(object)]
pub struct Curry {
    pub program: ClassInstance<ClvmPtr>,
    pub args: Vec<ClassInstance<ClvmPtr>>,
}
