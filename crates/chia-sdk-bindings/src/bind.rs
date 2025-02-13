use chia_protocol::Bytes;

use super::Result;

pub trait Bind<T> {
    fn bind(self) -> Result<T>;
}

impl Bind<String> for String {
    fn bind(self) -> Result<String> {
        Ok(self)
    }
}

#[cfg(feature = "napi")]
impl Bind<napi::bindgen_prelude::Uint8Array> for Bytes {
    fn bind(self) -> Result<napi::bindgen_prelude::Uint8Array> {
        Ok(napi::bindgen_prelude::Uint8Array::from(self.as_ref()))
    }
}

#[cfg(feature = "wasm")]
impl Bind<Vec<u8>> for Bytes {
    fn bind(self) -> Result<Vec<u8>> {
        Ok(self.into_inner())
    }
}
