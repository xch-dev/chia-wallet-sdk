use chia_protocol::Bytes;
use napi::bindgen_prelude::Uint8Array;

use super::Result;

pub trait Bind<T> {
    fn bind(self) -> Result<T>;
}

impl Bind<String> for String {
    fn bind(self) -> Result<String> {
        Ok(self)
    }
}

impl Bind<Uint8Array> for Bytes {
    fn bind(self) -> Result<Uint8Array> {
        Ok(Uint8Array::from(self.as_ref()))
    }
}
