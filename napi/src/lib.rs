#![allow(unsafe_code)]
#![allow(clippy::wildcard_imports)]

use napi::Env;

bindy_macro::bindy_napi!("bindings.json");
