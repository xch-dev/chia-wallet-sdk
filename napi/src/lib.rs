#![allow(unsafe_code)]
#![allow(clippy::wildcard_imports)]

use chia_sdk_bindings::*;
use napi::Env;

bindy_macro::bindy_napi!("bindings.json");
