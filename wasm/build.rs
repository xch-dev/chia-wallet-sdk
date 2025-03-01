fn main() {
    println!("cargo::rerun-if-changed=../bindings");
    println!("cargo::rerun-if-changed=../bindings.json");
}
