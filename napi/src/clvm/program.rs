use clvmr::NodePtr;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoJsWithClvm, IntoRust};

use super::{Clvm, CurriedProgram, Output, Pair, Puzzle};

#[napi]
pub struct Program {
    pub(crate) clvm: Reference<Clvm>,
    pub(crate) node_ptr: NodePtr,
    pub(crate) tree_hash: Option<Uint8Array>,
}

impl Program {
    pub fn new(clvm: Reference<Clvm>, node_ptr: NodePtr) -> Self {
        Self {
            clvm,
            node_ptr,
            tree_hash: None,
        }
    }
}

#[napi]
impl Program {
    #[napi(getter)]
    pub fn is_atom(&self) -> bool {
        self.node_ptr.is_atom()
    }

    #[napi(getter)]
    pub fn is_pair(&self) -> bool {
        self.node_ptr.is_pair()
    }

    #[napi(getter)]
    pub fn length(&self) -> Result<f64> {
        #[allow(clippy::cast_precision_loss)]
        Ok(self.clvm.0.length(self.node_ptr)? as f64)
    }

    #[napi(getter)]
    pub fn first(&self, env: Env) -> Result<Reference<Program>> {
        let first = self.clvm.0.first(self.node_ptr)?;
        Program::new(self.clvm.clone(env)?, first).into_reference(env)
    }

    #[napi(getter)]
    pub fn rest(&self, env: Env) -> Result<Reference<Program>> {
        let rest = self.clvm.0.rest(self.node_ptr)?;
        Program::new(self.clvm.clone(env)?, rest).into_reference(env)
    }

    #[napi]
    pub fn serialize(&mut self) -> Result<Uint8Array> {
        Ok(self.clvm.0.serialize(self.node_ptr)?.js()?)
    }

    #[napi]
    pub fn serialize_with_backrefs(&mut self) -> Result<Uint8Array> {
        Ok(self.clvm.0.serialize_with_backrefs(self.node_ptr)?.js()?)
    }

    #[napi]
    pub fn tree_hash(&mut self) -> Result<Uint8Array> {
        if let Some(tree_hash) = &self.tree_hash {
            return Ok(tree_hash.to_vec().into());
        }
        let tree_hash = self.clvm.0.tree_hash(self.node_ptr)?.js()?;
        self.tree_hash = Some(tree_hash.to_vec().into());
        Ok(tree_hash)
    }

    #[napi]
    pub fn to_number(&self) -> Result<Option<f64>> {
        Ok(self.clvm.0.as_f64(self.node_ptr)?)
    }

    #[napi]
    pub fn to_big_int(&self) -> Result<Option<BigInt>> {
        Ok(self
            .clvm
            .0
            .as_bigint(self.node_ptr)?
            .map(IntoJs::js)
            .transpose()?)
    }

    #[napi]
    pub fn to_string(&self) -> Result<Option<String>> {
        Ok(self.clvm.0.as_string(self.node_ptr)?)
    }

    #[napi]
    pub fn to_bool(&self) -> Result<Option<bool>> {
        Ok(self.clvm.0.as_bool(self.node_ptr)?)
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Option<Uint8Array>> {
        Ok(self
            .clvm
            .0
            .as_atom(self.node_ptr)?
            .map(IntoJs::js)
            .transpose()?)
    }

    #[napi]
    pub fn to_pair(&self, env: Env) -> Result<Option<Pair>> {
        let Some(pair) = self.clvm.0.as_pair(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program::new(self.clvm.clone(env)?, pair.0).into_reference(env)?,
            rest: Program::new(self.clvm.clone(env)?, pair.1).into_reference(env)?,
        }))
    }

    #[napi]
    pub fn to_list(&self, env: Env) -> Result<Option<Vec<Program>>> {
        let Some(list) = self.clvm.0.as_list(self.node_ptr)? else {
            return Ok(None);
        };

        let mut programs = Vec::new();

        for node_ptr in list {
            programs.push(Program::new(self.clvm.clone(env)?, node_ptr));
        }

        Ok(Some(programs))
    }

    #[napi]
    pub fn curry(&mut self, env: Env, args: Vec<Reference<Program>>) -> Result<Program> {
        let mut arg_ptrs = Vec::new();

        for arg in &args {
            arg_ptrs.push(arg.node_ptr);
        }

        Ok(Program::new(
            self.clvm.clone(env)?,
            self.clvm.0.curry(self.node_ptr, arg_ptrs)?,
        ))
    }

    #[napi]
    pub fn uncurry(&self, env: Env) -> Result<Option<CurriedProgram>> {
        let Some((program, args)) = self.clvm.0.uncurry(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(CurriedProgram {
            program: Program::new(self.clvm.clone(env)?, program).into_reference(env)?,
            args: args
                .iter()
                .map(|&node_ptr| Program::new(self.clvm.clone(env)?, node_ptr).into_reference(env))
                .collect::<Result<Vec<_>>>()?,
        }))
    }

    #[napi]
    pub fn run(
        &mut self,
        env: Env,
        solution: &Program,
        max_cost: BigInt,
        mempool_mode: bool,
    ) -> Result<Output> {
        let reduction = self.clvm.0.run(
            self.node_ptr,
            solution.node_ptr,
            max_cost.rust()?,
            mempool_mode,
        )?;

        Ok(Output {
            value: Program::new(self.clvm.clone(env)?, reduction.1).into_reference(env)?,
            cost: reduction.0.js()?,
        })
    }

    #[napi]
    pub fn puzzle(&self, env: Env) -> Result<Puzzle> {
        let puzzle = self.clvm.0.parse_puzzle(self.node_ptr)?;
        Ok(puzzle.js_with_clvm(env, &self.clvm)?)
    }
}
