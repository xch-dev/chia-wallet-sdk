use clvmr::NodePtr;
use napi::{bindgen_prelude::*, NapiRaw};
use napi_derive::napi;

use crate::{
    IntoJs, IntoRust, K1PublicKey, K1Signature, PublicKey, R1PublicKey, R1Signature, Signature,
};

pub type Value<'a> = Either16<
    f64,
    BigInt,
    bool,
    String,
    Uint8Array,
    ClassInstance<'a, Program>,
    ClassInstance<'a, PublicKey>,
    ClassInstance<'a, Signature>,
    ClassInstance<'a, K1PublicKey>,
    ClassInstance<'a, K1Signature>,
    ClassInstance<'a, R1PublicKey>,
    ClassInstance<'a, R1Signature>,
    ClassInstance<'a, Pair>,
    ClassInstance<'a, CurriedProgram>,
    Array,
    Null,
>;

#[napi]
pub struct Clvm(chia_sdk_bindings::Clvm);

#[napi]
impl Clvm {
    #[napi]
    pub fn alloc<'a>(&mut self, env: Env, this: This<'a>, value: Value<'a>) -> Result<Program> {
        let clvm = clvm(env, this)?;
        let node_ptr = alloc(env, clvm.clone(env)?, value)?;
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
}

#[napi]
pub struct Program {
    clvm: Reference<Clvm>,
    node_ptr: NodePtr,
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
        Program {
            clvm: self.clvm.clone(env)?,
            node_ptr: first,
        }
        .into_reference(env)
    }

    #[napi(getter)]
    pub fn rest(&self, env: Env) -> Result<Reference<Program>> {
        let rest = self.clvm.0.rest(self.node_ptr)?;
        Program {
            clvm: self.clvm.clone(env)?,
            node_ptr: rest,
        }
        .into_reference(env)
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
    pub fn tree_hash(&self) -> Result<Uint8Array> {
        Ok(self.clvm.0.tree_hash(self.node_ptr)?.js()?)
    }

    #[napi]
    pub fn as_number(&self) -> Result<Option<f64>> {
        Ok(self.clvm.0.as_f64(self.node_ptr)?)
    }

    #[napi]
    pub fn as_big_int(&self) -> Result<Option<BigInt>> {
        Ok(self
            .clvm
            .0
            .as_bigint(self.node_ptr)?
            .map(IntoJs::js)
            .transpose()?)
    }

    #[napi]
    pub fn as_string(&self) -> Result<Option<String>> {
        Ok(self.clvm.0.as_string(self.node_ptr)?)
    }

    #[napi]
    pub fn as_bool(&self) -> Result<Option<bool>> {
        Ok(self.clvm.0.as_bool(self.node_ptr)?)
    }

    #[napi]
    pub fn as_atom(&self) -> Result<Option<Uint8Array>> {
        Ok(self
            .clvm
            .0
            .as_atom(self.node_ptr)?
            .map(IntoJs::js)
            .transpose()?)
    }

    #[napi]
    pub fn as_pair(&self, env: Env) -> Result<Option<Pair>> {
        let Some(pair) = self.clvm.0.as_pair(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program {
                clvm: self.clvm.clone(env)?,
                node_ptr: pair.0,
            }
            .into_reference(env)?,
            second: Program {
                clvm: self.clvm.clone(env)?,
                node_ptr: pair.1,
            }
            .into_reference(env)?,
        }))
    }

    #[napi]
    pub fn as_list(&self, env: Env) -> Result<Option<Vec<Program>>> {
        let Some(list) = self.clvm.0.as_list(self.node_ptr)? else {
            return Ok(None);
        };

        let mut programs = Vec::new();

        for node_ptr in list {
            programs.push(Program {
                clvm: self.clvm.clone(env)?,
                node_ptr,
            });
        }

        Ok(Some(programs))
    }

    #[napi]
    pub fn uncurry(&self, env: Env) -> Result<Option<CurriedProgram>> {
        let Some((program, args)) = self.clvm.0.uncurry(self.node_ptr)? else {
            return Ok(None);
        };

        Ok(Some(CurriedProgram {
            program: Program {
                clvm: self.clvm.clone(env)?,
                node_ptr: program,
            }
            .into_reference(env)?,
            args: args
                .iter()
                .map(|&node_ptr| {
                    Program {
                        clvm: self.clvm.clone(env)?,
                        node_ptr,
                    }
                    .into_reference(env)
                })
                .collect::<Result<Vec<_>>>()?,
        }))
    }
}

#[napi]
pub struct Pair {
    first: Reference<Program>,
    second: Reference<Program>,
}

#[napi]
impl Pair {
    #[napi(constructor)]
    pub fn new(first: Reference<Program>, second: Reference<Program>) -> Self {
        Self { first, second }
    }

    #[napi(getter)]
    pub fn first(&self, env: Env) -> Result<Reference<Program>> {
        self.first.clone(env)
    }

    #[napi(getter)]
    pub fn second(&self, env: Env) -> Result<Reference<Program>> {
        self.second.clone(env)
    }
}

#[napi]
pub struct CurriedProgram {
    program: Reference<Program>,
    args: Vec<Reference<Program>>,
}

#[napi]
impl CurriedProgram {
    #[napi(constructor)]
    pub fn new(program: Reference<Program>, args: Vec<Reference<Program>>) -> Self {
        Self { program, args }
    }

    #[napi(getter)]
    pub fn program(&self, env: Env) -> Result<Reference<Program>> {
        self.program.clone(env)
    }

    #[napi(getter)]
    pub fn args(&self, env: Env) -> Result<Vec<Reference<Program>>> {
        self.args
            .iter()
            .map(|arg| arg.clone(env))
            .collect::<Result<Vec<_>>>()
    }
}

fn clvm(env: Env, this: This<'_>) -> Result<Reference<Clvm>> {
    Ok(unsafe { Reference::from_napi_value(env.raw(), this.object.raw())? })
}

fn alloc<'a>(env: Env, mut clvm: Reference<Clvm>, value: Value<'a>) -> Result<NodePtr> {
    match value {
        Value::A(value) => Ok(clvm.0.new_f64(value)?),
        Value::B(value) => Ok(clvm.0.new_bigint(value.rust()?)?),
        Value::C(value) => Ok(clvm.0.new_bool(value)?),
        Value::D(value) => Ok(clvm.0.new_string(value)?),
        Value::E(value) => Ok(clvm.0.new_atom(value.to_vec().into())?),
        Value::F(value) => Ok(value.node_ptr),
        Value::G(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::H(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::I(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::J(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::K(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::L(value) => Ok(clvm.0.new_atom(value.to_bytes()?.to_vec().into())?),
        Value::M(value) => Ok(clvm
            .0
            .new_pair(value.first.node_ptr, value.second.node_ptr)?),
        Value::N(value) => {
            let mut args = Vec::new();

            for arg in &value.args {
                args.push(arg.node_ptr);
            }

            Ok(clvm.0.curry(value.program.node_ptr, args)?)
        }
        Value::O(value) => {
            let mut list = Vec::new();

            for index in 0..value.len() {
                let item = value.get::<Value<'a>>(index)?.unwrap();
                list.push(alloc(env, clvm.clone(env)?, item)?);
            }

            Ok(clvm.0.new_list(list)?)
        }
        Value::P(_) => Ok(NodePtr::NIL),
    }
}
