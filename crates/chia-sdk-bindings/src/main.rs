use std::fs;

use chia_sdk_bindings::generate_type_stubs;

fn main() {
    let type_stubs = generate_type_stubs();
    fs::write("index.d.ts", type_stubs).unwrap();
}
