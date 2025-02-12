mod bind;
mod error;
mod typings;
mod unbind;

pub(crate) use bind::*;
pub(crate) use error::*;
pub(crate) use typings::*;
pub(crate) use unbind::*;

pub use typings::generate_type_stubs;

use chia_protocol::BytesImpl;
use napi::bindgen_prelude::Uint8Array;

impl<const N: usize> Unbind for BytesImpl<N> {
    type Bound = Uint8Array;

    fn unbind(value: Self::Bound) -> Result<Self> {
        let bytes = value.as_ref();

        if bytes.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: bytes.len(),
            });
        }

        Ok(BytesImpl::new(bytes.try_into().unwrap()))
    }
}

impl Bind<String> for String {
    fn bind(self) -> Result<String> {
        Ok(self)
    }
}

impl Unbind for String {
    type Bound = String;

    fn unbind(value: Self::Bound) -> Result<Self> {
        Ok(value)
    }
}
