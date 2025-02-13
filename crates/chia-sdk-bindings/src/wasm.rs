use chia_protocol::{Bytes, BytesImpl};

use crate::{Error, Result, Unbind};

impl Unbind for Bytes {
    type Bound = Vec<u8>;

    fn unbind(value: Self::Bound) -> Result<Self> {
        Ok(Bytes::new(value))
    }
}

impl<const N: usize> Unbind for BytesImpl<N> {
    type Bound = Vec<u8>;

    fn unbind(value: Self::Bound) -> Result<Self> {
        if value.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: value.len(),
            });
        }

        Ok(BytesImpl::new(value.try_into().unwrap()))
    }
}
