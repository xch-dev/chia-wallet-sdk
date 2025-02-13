#![allow(clippy::needless_pass_by_value)]

mod bind;
mod error;
mod unbind;
mod utils;

pub use bind::*;
pub use error::*;
pub use unbind::*;
pub use utils::*;

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
