use chia_protocol::{Bytes, BytesImpl};
use napi::bindgen_prelude::*;

use super::{Error, Result};

pub trait Unbind: Sized {
    type Bound;

    fn unbind(value: Self::Bound) -> Result<Self>;
}

impl Unbind for String {
    type Bound = String;

    fn unbind(value: Self::Bound) -> Result<Self> {
        Ok(value)
    }
}

impl Unbind for Bytes {
    type Bound = Uint8Array;

    fn unbind(value: Self::Bound) -> Result<Self> {
        Ok(Bytes::new(value.to_vec()))
    }
}

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
