#![allow(unsafe_code)]

use napi_derive::napi;

pub use chia_sdk_bindings::*;

#[napi]
pub fn hello_world() -> String {
    "Hello, world!".to_string()
}
