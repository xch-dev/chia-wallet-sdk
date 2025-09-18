use std::fs;

const STUBS: &str = bindy_macro::bindy_pyo3_stubs!("bindings.json");

fn main() {
    fs::write("pyo3/chia_wallet_sdk.pyi", STUBS).unwrap();
}
