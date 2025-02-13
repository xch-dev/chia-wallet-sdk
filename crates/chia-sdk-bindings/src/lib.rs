#![allow(clippy::needless_pass_by_value)]

mod bind;
mod bindings;
mod error;
mod unbind;

pub use bind::*;
pub use bindings::*;
pub use error::*;
pub use unbind::*;

#[cfg(all(feature = "napi", not(feature = "wasm")))]
mod napi;

#[cfg(all(feature = "wasm", not(feature = "napi")))]
mod wasm;

pub mod prelude {
    pub use super::bind::*;
    pub use super::error::*;
    pub use super::unbind::*;

    pub use chia_protocol::{Bytes, Bytes32};
}
