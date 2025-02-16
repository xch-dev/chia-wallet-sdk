use chia_sdk_bindings::{AddressInfo, Bytes, BytesImpl, Error, Result};

pub trait IntoRust<T> {
    fn rust(self) -> Result<T>;
}

pub trait IntoPy {
    type Py;

    fn py(self) -> Result<Self::Py>;
}

impl<const N: usize> IntoRust<BytesImpl<N>> for Vec<u8> {
    fn rust(self) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: self.len(),
            });
        }
        Ok(BytesImpl::new(self.try_into().unwrap()))
    }
}

impl<const N: usize> IntoPy for BytesImpl<N> {
    type Py = Vec<u8>;

    fn py(self) -> Result<Self::Py> {
        Ok(self.into())
    }
}

impl IntoRust<Bytes> for Vec<u8> {
    fn rust(self) -> Result<Bytes> {
        Ok(Bytes::new(self))
    }
}

impl IntoPy for Bytes {
    type Py = Vec<u8>;

    fn py(self) -> Result<Self::Py> {
        Ok(self.into())
    }
}

impl IntoRust<AddressInfo> for crate::AddressInfo {
    fn rust(self) -> Result<AddressInfo> {
        Ok(AddressInfo {
            puzzle_hash: self.puzzle_hash.rust()?,
            prefix: self.prefix,
        })
    }
}

impl IntoPy for AddressInfo {
    type Py = crate::AddressInfo;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            puzzle_hash: self.puzzle_hash.py()?,
            prefix: self.prefix,
        })
    }
}
