use napi::bindgen_prelude::Uint8Array;

pub(crate) trait Typings {
    fn typings() -> String;
}

impl Typings for String {
    fn typings() -> String {
        "string".to_string()
    }
}

impl Typings for Uint8Array {
    fn typings() -> String {
        "Uint8Array".to_string()
    }
}

pub(crate) struct TypedFn {
    pub(crate) name: &'static str,
    pub(crate) typings: fn() -> String,
}

impl TypedFn {
    pub(crate) const fn new(name: &'static str, typings: fn() -> String) -> Self {
        TypedFn { name, typings }
    }
}

inventory::collect!(TypedFn);

pub fn generate_type_stubs() -> String {
    let mut items = inventory::iter::<TypedFn>().collect::<Vec<_>>();
    items.sort_by_key(|item| item.name);

    items
        .iter()
        .map(|item| (item.typings)())
        .collect::<Vec<_>>()
        .join("\n")
}
