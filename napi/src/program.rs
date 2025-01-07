use std::{num::TryFromIntError, string::FromUtf8Error};

use chia::{
    bls,
    clvm_traits::{ClvmDecoder, FromClvm},
    clvm_utils::{tree_hash, CurriedProgram},
};
use clvmr::{
    serde::{node_to_bytes, node_to_bytes_backrefs},
    Allocator, NodePtr, SExp,
};
use napi::bindgen_prelude::*;

use crate::{
    traits::{js_err, IntoJs},
    ClvmAllocator, PublicKey, Signature,
};

#[napi]
pub struct Program {
    pub(crate) ctx: Reference<ClvmAllocator>,
    pub(crate) ptr: NodePtr,
}

impl Program {
    pub(crate) fn new(ctx: Reference<ClvmAllocator>, ptr: NodePtr) -> Self {
        Self { ctx, ptr }
    }

    fn alloc(&self) -> &Allocator {
        &self.ctx.0.allocator
    }
}

#[napi]
impl Program {
    #[napi]
    pub fn is_atom(&self) -> bool {
        self.ptr.is_atom()
    }

    #[napi]
    pub fn is_pair(&self) -> bool {
        self.ptr.is_pair()
    }

    #[napi]
    pub fn tree_hash(&self) -> Result<Uint8Array> {
        tree_hash(self.alloc(), self.ptr).to_bytes().into_js()
    }

    #[napi]
    pub fn serialize(&self) -> Result<Uint8Array> {
        let bytes = node_to_bytes(self.alloc(), self.ptr)?;
        bytes.into_js()
    }

    #[napi]
    pub fn serialize_with_backrefs(&self) -> Result<Uint8Array> {
        let bytes = node_to_bytes_backrefs(self.alloc(), self.ptr)?;
        bytes.into_js()
    }

    #[napi]
    pub fn length(&self) -> Result<Option<u32>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => Ok(Some(self.alloc().atom_len(self.ptr).try_into().map_err(
                |error: TryFromIntError| Error::from_reason(error.to_string()),
            )?)),
            SExp::Pair(..) => Ok(None),
        }
    }

    #[napi]
    pub fn to_atom(&self) -> Result<Option<Uint8Array>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => Ok(Some(
                self.alloc().atom(self.ptr).as_ref().to_vec().into_js()?,
            )),
            SExp::Pair(..) => Ok(None),
        }
    }

    #[napi(ts_return_type = "[Program, Program] | null")]
    pub fn to_pair(&self, env: Env) -> Result<Option<Array>> {
        let SExp::Pair(first, rest) = self.alloc().sexp(self.ptr) else {
            return Ok(None);
        };

        let mut array = env.create_array(2)?;

        array.set(
            0,
            Program::new(self.ctx.clone(env)?, first).into_instance(env)?,
        )?;

        array.set(
            1,
            Program::new(self.ctx.clone(env)?, rest).into_instance(env)?,
        )?;

        Ok(Some(array))
    }

    #[napi(getter)]
    pub fn first(&self, env: Env) -> Result<Program> {
        let SExp::Pair(first, _rest) = self.alloc().sexp(self.ptr) else {
            return Err(Error::from_reason("Cannot call first on an atom"));
        };
        Ok(Program::new(self.ctx.clone(env)?, first))
    }

    #[napi(getter)]
    pub fn rest(&self, env: Env) -> Result<Program> {
        let SExp::Pair(_first, rest) = self.alloc().sexp(self.ptr) else {
            return Err(Error::from_reason("Cannot call rest on an atom"));
        };
        Ok(Program::new(self.ctx.clone(env)?, rest))
    }

    #[napi]
    pub fn to_list(&self, env: Env) -> Result<Vec<ClassInstance<Program>>> {
        Vec::<NodePtr>::from_clvm(self.alloc(), self.ptr)
            .map_err(js_err)?
            .into_iter()
            .map(|ptr| Program::new(self.ctx.clone(env)?, ptr).into_instance(env))
            .collect()
    }

    #[napi]
    pub fn uncurry(&self, env: Env) -> Result<Option<Curry>> {
        let Ok(value) = CurriedProgram::<NodePtr, NodePtr>::from_clvm(self.alloc(), self.ptr)
        else {
            return Ok(None);
        };

        let mut args = Vec::new();
        let mut args_ptr = value.args;

        while let Ok((first, rest)) = self.alloc().decode_curried_arg(&args_ptr) {
            args.push(Program::new(self.ctx.clone(env)?, first).into_instance(env)?);
            args_ptr = rest;
        }

        if self.alloc().small_number(args_ptr) != Some(1) {
            return Ok(None);
        }

        Ok(Some(Curry {
            program: Program::new(self.ctx.clone(env)?, value.program).into_instance(env)?,
            args,
        }))
    }

    #[napi]
    pub fn to_string(&self) -> Result<Option<String>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => Ok(Some(
                String::from_utf8(self.alloc().atom(self.ptr).as_ref().to_vec())
                    .map_err(|error: FromUtf8Error| Error::from_reason(error.to_string()))?,
            )),
            SExp::Pair(..) => Ok(None),
        }
    }

    #[napi]
    pub fn to_small_number(&self) -> Option<u32> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => self.alloc().small_number(self.ptr),
            SExp::Pair(..) => None,
        }
    }

    #[napi]
    pub fn to_big_int(&self) -> Result<Option<BigInt>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => Ok(Some(self.alloc().number(self.ptr).into_js()?)),
            SExp::Pair(..) => Ok(None),
        }
    }

    #[napi]
    pub fn to_public_key(&self) -> Result<Option<PublicKey>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => {
                let atom = self.alloc().atom(self.ptr);
                Ok(Some(PublicKey(
                    bls::PublicKey::from_bytes(
                        &atom
                            .as_ref()
                            .try_into()
                            .map_err(|_| Error::from_reason("Invalid public key"))?,
                    )
                    .map_err(js_err)?,
                )))
            }
            SExp::Pair(..) => Ok(None),
        }
    }

    #[napi]
    pub fn to_signature(&self) -> Result<Option<Signature>> {
        match self.alloc().sexp(self.ptr) {
            SExp::Atom => {
                let atom = self.alloc().atom(self.ptr);
                Ok(Some(Signature(
                    bls::Signature::from_bytes(
                        &atom
                            .as_ref()
                            .try_into()
                            .map_err(|_| Error::from_reason("Invalid signature"))?,
                    )
                    .map_err(js_err)?,
                )))
            }
            SExp::Pair(..) => Ok(None),
        }
    }
}

#[napi(object)]
pub struct Curry {
    pub program: ClassInstance<Program>,
    pub args: Vec<ClassInstance<Program>>,
}
