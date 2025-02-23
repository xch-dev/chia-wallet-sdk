use clvmr::NodePtr;
use napi::{bindgen_prelude::*, NapiRaw};

use crate::{IntoRust, K1PublicKey, K1Signature, PublicKey, R1PublicKey, R1Signature, Signature};

use super::{Clvm, Program, Spend};

pub type Value<'a> = Either14<
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
    Array,
    Null,
>;

pub fn clvm(env: Env, this: This<'_>) -> Result<Reference<Clvm>> {
    Ok(unsafe { Reference::from_napi_value(env.raw(), this.object.raw())? })
}

pub fn alloc<'a>(env: Env, mut clvm: Reference<Clvm>, value: Value<'a>) -> Result<NodePtr> {
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
        Value::M(value) => {
            let mut list = Vec::new();

            for index in 0..value.len() {
                let item = value.get::<Value<'a>>(index)?.unwrap();
                list.push(alloc(env, clvm.clone(env)?, item)?);
            }

            Ok(clvm.0.new_list(list)?)
        }
        Value::N(_) => Ok(NodePtr::NIL),
    }
}

pub fn spend_to_js(
    env: Env,
    clvm: Reference<Clvm>,
    spend: chia_sdk_bindings::Spend,
) -> Result<Spend> {
    Ok(Spend {
        puzzle: Program::new(clvm.clone(env)?, spend.puzzle).into_reference(env)?,
        solution: Program::new(clvm.clone(env)?, spend.solution).into_reference(env)?,
    })
}
