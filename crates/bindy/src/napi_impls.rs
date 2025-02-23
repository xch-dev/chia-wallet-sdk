use chia_protocol::BytesImpl;
use napi::bindgen_prelude::*;

use crate::{Error, FromRust, IntoRust, Result};

impl From<Error> for napi::Error {
    fn from(_value: Error) -> Self {
        napi::Error::new(napi::Status::GenericFailure, "error")
    }
}

pub struct NapiParamContext;

pub struct NapiStructContext(pub Env);

pub struct NapiReturnContext(pub Env);

impl<T> FromRust<T, NapiStructContext> for Reference<T>
where
    T: JavaScriptClassExt,
{
    fn from_rust(value: T, context: &NapiStructContext) -> Result<Self> {
        let env = context.0;
        Ok(value.into_reference(env)?)
    }
}

impl<T, U> IntoRust<T, NapiParamContext> for ClassInstance<'_, U>
where
    U: Clone + IntoRust<T, NapiParamContext>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        std::ops::Deref::deref(&self).clone().into_rust(context)
    }
}

impl FromRust<(), NapiReturnContext> for napi::JsUndefined {
    fn from_rust(_value: (), context: &NapiReturnContext) -> Result<Self> {
        Ok(context.0.get_undefined()?)
    }
}

impl<T, const N: usize> FromRust<BytesImpl<N>, T> for Uint8Array {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T> for Uint8Array {
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        let bytes = self.to_vec();

        if bytes.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: bytes.len(),
            });
        }

        Ok(bytes.try_into().unwrap())
    }
}
