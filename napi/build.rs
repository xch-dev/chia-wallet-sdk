#[allow(unused_extern_crates)]
extern crate napi_build;

fn main() {
    println!("cargo::rerun-if-changed=../bindings");
    println!("cargo::rerun-if-changed=../bindings.json");
    napi_build::setup();
}
