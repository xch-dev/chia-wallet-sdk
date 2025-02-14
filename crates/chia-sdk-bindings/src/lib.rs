#![allow(clippy::needless_pass_by_value)]

mod bindings;
mod error;

pub use bindings::*;
pub use error::*;

pub mod prelude {
    pub use super::error::*;

    pub use chia_protocol::{Bytes, Bytes32, BytesImpl};

    pub mod rust {
        pub use crate::SecretKey;
        pub use chia_sdk_utils::AddressInfo;
    }
}
