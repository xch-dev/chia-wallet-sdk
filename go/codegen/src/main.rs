use std::collections::HashSet;
use std::path::Path;
use std::{env, fs};

use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde::Deserialize;

use chia_sdk_bindings::CONSTANTS;

fn validate_identifier(name: &str) {
    if name.is_empty() {
        panic!("binding name must not be empty");
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        panic!("invalid identifier: {name:?}");
    }
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            panic!("invalid identifier: {name:?}");
        }
    }
}

// ── JSON schema (mirrors bindy-macro) ──────────────────────────────────────

#[derive(Deserialize)]
struct Bindy {
    entrypoint: String,
    #[serde(default)]
    type_groups: IndexMap<String, Vec<String>>,
    #[serde(default)]
    shared: IndexMap<String, String>,
    #[serde(default)]
    go: IndexMap<String, String>,
    #[allow(dead_code)]
    #[serde(default)]
    clvm_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Binding {
    Class {
        #[serde(default)]
        doc: Option<String>,
        #[serde(default)]
        new: bool,
        #[serde(default)]
        fields: IndexMap<String, String>,
        #[serde(default)]
        methods: IndexMap<String, Method>,
        #[serde(default)]
        remote: bool,
        #[allow(dead_code)]
        #[serde(default)]
        no_wasm: bool,
    },
    Enum {
        #[serde(default)]
        doc: Option<String>,
        values: Vec<String>,
    },
    Function {
        #[serde(default)]
        doc: Option<String>,
        #[serde(default)]
        args: IndexMap<String, String>,
        #[serde(rename = "return")]
        ret: Option<String>,
    },
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct Method {
    #[serde(default)]
    doc: Option<String>,
    #[serde(rename = "type")]
    kind: MethodKind,
    args: IndexMap<String, String>,
    #[serde(rename = "return")]
    ret: Option<String>,
    #[serde(default)]
    stub_only: bool,
    #[allow(dead_code)]
    #[serde(default)]
    no_wasm: bool,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MethodKind {
    #[default]
    Normal,
    Async,
    ToString,
    Static,
    Factory,
    AsyncFactory,
    Constructor,
}

// ── Type classification ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum FfiKind {
    Void,
    Bool,
    Prim(String),
    Bytes,
    Str,
    BigInt,
    Class(String),
    Enum(String),
    Opt(Box<FfiKind>),
    List(Box<FfiKind>),
}

fn apply_mappings(ty: &str, mappings: &IndexMap<String, String>) -> String {
    if let Some(mapped) = mappings.get(ty) {
        return mapped.clone();
    }
    if let (Some(start), Some(end)) = (ty.find('<'), ty.rfind('>')) {
        let base = &ty[..start];
        let inner = &ty[start + 1..end];
        let params: Vec<&str> = inner.split(',').map(str::trim).collect();
        let mapped_params: Vec<String> = params
            .iter()
            .map(|p| apply_mappings(p, mappings))
            .collect();
        let mapped_base = mappings.get(base).map_or(base, String::as_str);
        format!("{}<{}>", mapped_base, mapped_params.join(", "))
    } else {
        ty.to_string()
    }
}

fn classify(ty: &str, mappings: &IndexMap<String, String>, classes: &HashSet<String>, enums: &HashSet<String>) -> FfiKind {
    let mapped = apply_mappings(ty, mappings);
    classify_mapped(&mapped, mappings, classes, enums)
}

fn classify_mapped(
    mapped: &str,
    mappings: &IndexMap<String, String>,
    classes: &HashSet<String>,
    enums: &HashSet<String>,
) -> FfiKind {
    match mapped {
        "()" => FfiKind::Void,
        "bool" => FfiKind::Bool,
        "String" => FfiKind::Str,
        "Vec<u8>" => FfiKind::Bytes,
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "usize" | "f32"
        | "f64" => FfiKind::Prim(mapped.to_string()),
        "u128" | "num_bigint::BigInt" => FfiKind::BigInt,
        _ if mapped.starts_with("Option<") => {
            let inner = &mapped[7..mapped.len() - 1];
            FfiKind::Opt(Box::new(classify_mapped(inner, mappings, classes, enums)))
        }
        _ if mapped.starts_with("Vec<") => {
            let inner = &mapped[4..mapped.len() - 1];
            FfiKind::List(Box::new(classify_mapped(inner, mappings, classes, enums)))
        }
        _ if enums.contains(mapped) => FfiKind::Enum(mapped.to_string()),
        _ if classes.contains(mapped) => FfiKind::Class(mapped.to_string()),
        _ => {
            // Unknown types are treated as opaque class handles.
            FfiKind::Class(mapped.to_string())
        }
    }
}

// ── Loading ────────────────────────────────────────────────────────────────

fn load_bindings(root: &Path) -> (Bindy, IndexMap<String, Binding>) {
    let source = fs::read_to_string(root.join("bindings.json"))
        .expect("failed to read bindings.json");
    let bindy: Bindy =
        serde_json::from_str(&source).expect("failed to parse bindings.json");

    let mut bindings = IndexMap::new();
    let mut dir: Vec<_> = fs::read_dir(root.join("bindings"))
        .expect("failed to read bindings directory")
        .map(|p| p.expect("failed to read directory entry"))
        .collect();
    dir.sort_by_key(|p| {
        p.path()
            .file_name()
            .expect("directory entry has no file name")
            .to_str()
            .expect("file name is not valid UTF-8")
            .to_string()
    });

    for path in dir {
        if path.path().extension().unwrap_or_default() == "json" {
            let source = fs::read_to_string(path.path())
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.path().display()));
            let contents: IndexMap<String, Binding> = serde_json::from_str(&source)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.path().display()));
            bindings.extend(contents);
        }
    }

    // Inject Constants methods
    if let Some(Binding::Class { methods, .. }) = bindings.get_mut("Constants") {
        for &name in CONSTANTS {
            methods.insert(
                name.to_string(),
                Method {
                    doc: None,
                    kind: MethodKind::Static,
                    args: IndexMap::new(),
                    ret: Some("SerializedProgram".to_string()),
                    stub_only: false,
                    no_wasm: false,
                },
            );
            methods.insert(
                format!("{name}_hash"),
                Method {
                    doc: None,
                    kind: MethodKind::Static,
                    args: IndexMap::new(),
                    ret: Some("TreeHash".to_string()),
                    stub_only: false,
                    no_wasm: false,
                },
            );
        }
    }

    // Inject Clvm constant methods
    if let Some(Binding::Class { methods, .. }) = bindings.get_mut("Clvm") {
        for &name in CONSTANTS {
            methods.insert(
                name.to_string(),
                Method {
                    doc: None,
                    kind: MethodKind::Normal,
                    args: IndexMap::new(),
                    ret: Some("Program".to_string()),
                    stub_only: false,
                    no_wasm: false,
                },
            );
        }
    }

    (bindy, bindings)
}

fn build_mappings(bindy: &Bindy) -> IndexMap<String, String> {
    let mut mappings = bindy.go.clone();
    for (name, value) in &bindy.shared {
        if !mappings.contains_key(name) {
            mappings.insert(name.clone(), value.clone());
        }
    }
    for (name, group) in &bindy.type_groups {
        if let Some(value) = mappings.shift_remove(name) {
            for ty in group {
                if !mappings.contains_key(ty) {
                    mappings.insert(ty.clone(), value.clone());
                }
            }
        }
    }
    mappings
}

// ── Rust code generation helpers ───────────────────────────────────────────

struct RustGen {
    out: String,
    entrypoint: String,
    mappings: IndexMap<String, String>,
    classes: HashSet<String>,
    enums: HashSet<String>,
}

impl RustGen {
    fn new(entrypoint: &str, mappings: IndexMap<String, String>, classes: HashSet<String>, enums: HashSet<String>) -> Self {
        Self {
            out: String::new(),
            entrypoint: entrypoint.to_string(),
            mappings,
            classes,
            enums,
        }
    }

    fn classify(&self, ty: &str) -> FfiKind {
        classify(ty, &self.mappings, &self.classes, &self.enums)
    }

    fn c_param_decl(&self, name: &str, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => format!("{name}: i32"),
            FfiKind::Prim(t) => format!("{name}: {t}"),
            FfiKind::Bytes => format!("{name}_ptr: *const u8, {name}_len: usize"),
            FfiKind::Str => format!("{name}: *const std::ffi::c_char"),
            FfiKind::BigInt => format!("{name}_ptr: *const u8, {name}_len: usize"),
            FfiKind::Class(_) | FfiKind::Enum(_) => format!("{name}: *const std::ffi::c_void"),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Bytes => format!("{name}_ptr: *const u8, {name}_len: usize"),
                FfiKind::Str => format!("{name}: *const std::ffi::c_char"),
                FfiKind::Bool => format!("{name}: i32, {name}_is_some: i32"),
                FfiKind::Prim(t) => format!("{name}: {t}, {name}_is_some: i32"),
                FfiKind::BigInt => {
                    format!("{name}_ptr: *const u8, {name}_len: usize")
                }
                // Class, List, and other complex types use opaque pointers (null = None)
                _ => format!("{name}: *const std::ffi::c_void"),
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => format!("{name}_ptrs: *const *const std::ffi::c_void, {name}_len: usize"),
                FfiKind::Bytes => format!("{name}_ptrs: *const *const u8, {name}_lens: *const usize, {name}_count: usize"),
                _ => format!("{name}: *const std::ffi::c_void"),
            },
        }
    }

    fn c_out_decl(&self, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => "out: *mut i32".to_string(),
            FfiKind::Prim(t) => format!("out: *mut {t}"),
            FfiKind::Bytes => "out_ptr: *mut *mut u8, out_len: *mut usize".to_string(),
            FfiKind::Str => "out: *mut *mut std::ffi::c_char".to_string(),
            FfiKind::BigInt => "out_ptr: *mut *mut u8, out_len: *mut usize".to_string(),
            FfiKind::Class(_) | FfiKind::Enum(_) => "out: *mut *mut std::ffi::c_void".to_string(),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => "out: *mut *mut std::ffi::c_void".to_string(),
                FfiKind::Bytes => "out_ptr: *mut *mut u8, out_len: *mut usize".to_string(),
                FfiKind::Str => "out: *mut *mut std::ffi::c_char".to_string(),
                FfiKind::Bool => "out: *mut i32, out_is_some: *mut i32".to_string(),
                FfiKind::Prim(t) => format!("out: *mut {t}, out_is_some: *mut i32"),
                FfiKind::BigInt => "out_ptr: *mut *mut u8, out_len: *mut usize".to_string(),
                // Complex types (List, Class, etc.) use opaque pointer (null = None)
                _ => "out: *mut *mut std::ffi::c_void".to_string(),
            },
            FfiKind::List(_) => "out: *mut *mut std::ffi::c_void".to_string(),
        }
    }

    fn param_to_rust(&self, name: &str, kind: &FfiKind, orig_type: &str) -> String {
        let ep = &self.entrypoint;
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => format!(
                "bindy::IntoRust::<_, _, bindy::Go>::into_rust({name} != 0, &bindy::GoContext)?"
            ),
            FfiKind::Prim(_) => format!(
                "bindy::IntoRust::<_, _, bindy::Go>::into_rust({name}, &bindy::GoContext)?"
            ),
            FfiKind::Bytes => format!(
                "{{ if {name}_ptr.is_null() {{ return Err(bindy::Error::Custom(\
                    format!(\"{name} must not be null\"))); }} \
                    bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                    std::slice::from_raw_parts({name}_ptr, {name}_len).to_vec(), &bindy::GoContext)? }}"
            ),
            FfiKind::Str => format!(
                "{{ if {name}.is_null() {{ return Err(bindy::Error::Custom(\
                    format!(\"{name} must not be null\"))); }} \
                    bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                    std::ffi::CStr::from_ptr({name}).to_str()\
                    .map_err(|e| bindy::Error::Custom(e.to_string()))?.to_string(), &bindy::GoContext)? }}"
            ),
            FfiKind::BigInt => format!(
                "{{ if {name}_ptr.is_null() {{ return Err(bindy::Error::Custom(\
                    format!(\"{name} must not be null\"))); }} \
                    let bytes = std::slice::from_raw_parts({name}_ptr, {name}_len); \
                   let big = num_bigint::BigInt::from_signed_bytes_be(bytes); \
                   bindy::IntoRust::<_, _, bindy::Go>::into_rust(big, &bindy::GoContext)? }}"
            ),
            FfiKind::Class(cls) | FfiKind::Enum(cls) => {
                format!(
                    "{{ if ({name}).is_null() {{ return Err(bindy::Error::Custom(\
                        format!(\"{name} must not be null\"))); }} \
                        (*(({name}) as *const {ep}::{cls})).clone() }}"
                )
            }
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => format!(
                    "if {name}.is_null() {{ None }} else {{ Some((*(({name}) as *const {ep}::{cls})).clone()) }}"
                ),
                FfiKind::Bytes => format!(
                    "if {name}_ptr.is_null() {{ None }} else {{ \
                        Some(bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                            std::slice::from_raw_parts({name}_ptr, {name}_len).to_vec(), &bindy::GoContext)?) }}"
                ),
                FfiKind::Str => format!(
                    "if {name}.is_null() {{ None }} else {{ \
                        Some(bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                            std::ffi::CStr::from_ptr({name}).to_str()\
                            .map_err(|e| bindy::Error::Custom(e.to_string()))?.to_string(), &bindy::GoContext)?) }}"
                ),
                FfiKind::Bool => format!(
                    "if {name}_is_some != 0 {{ Some({name} != 0) }} else {{ None }}"
                ),
                FfiKind::Prim(_) => format!(
                    "if {name}_is_some != 0 {{ \
                        Some(bindy::IntoRust::<_, _, bindy::Go>::into_rust({name}, &bindy::GoContext)?) \
                    }} else {{ None }}"
                ),
                FfiKind::BigInt => format!(
                    "if {name}_ptr.is_null() {{ None }} else {{ \
                        Some(bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                            std::slice::from_raw_parts({name}_ptr, {name}_len).to_vec(), &bindy::GoContext)?) }}"
                ),
                FfiKind::List(_) => {
                    // Extract inner type from Option<Vec<X>> -> Vec<X>
                    let inner_type = if orig_type.starts_with("Option<") {
                        &orig_type[7..orig_type.len() - 1]
                    } else {
                        orig_type
                    };
                    // Qualify the inner list element type
                    let qualified = if inner_type.starts_with("Vec<") {
                        let elem = &inner_type[4..inner_type.len() - 1];
                        format!("Vec<{ep}::{elem}>")
                    } else {
                        format!("{ep}::{inner_type}")
                    };
                    format!(
                        "if {name}.is_null() {{ None }} else {{ \
                            Some((*({name} as *const {qualified})).clone()) }}"
                    )
                },
                // Other complex types use opaque pointer (null = None)
                _ => {
                    let inner_type = if orig_type.starts_with("Option<") {
                        &orig_type[7..orig_type.len() - 1]
                    } else {
                        orig_type
                    };
                    format!(
                        "if {name}.is_null() {{ None }} else {{ \
                            Some((*({name} as *const {ep}::{inner_type})).clone()) }}"
                    )
                },
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => format!(
                    "{{ if {name}_ptrs.is_null() {{ return Err(bindy::Error::Custom(\
                        format!(\"{name} must not be null\"))); }} \
                       let ptrs = std::slice::from_raw_parts({name}_ptrs, {name}_len); \
                       ptrs.iter().map(|p| {{ if (*p).is_null() {{ return Err(bindy::Error::Custom(\
                           format!(\"{name} element must not be null\"))); }} \
                           Ok((*((*p) as *const {ep}::{cls})).clone()) \
                       }}).collect::<bindy::Result<Vec<_>>>()? }}"
                ),
                FfiKind::Bytes => format!(
                    "{{ if {name}_ptrs.is_null() {{ return Err(bindy::Error::Custom(\
                        format!(\"{name} must not be null\"))); }} \
                       let ptrs = std::slice::from_raw_parts({name}_ptrs, {name}_count); \
                       let lens = std::slice::from_raw_parts({name}_lens, {name}_count); \
                       ptrs.iter().zip(lens.iter()).map(|(p, l)| \
                           bindy::IntoRust::<_, _, bindy::Go>::into_rust(\
                               std::slice::from_raw_parts(*p, *l).to_vec(), &bindy::GoContext) \
                       ).collect::<bindy::Result<Vec<_>>>()? }}"
                ),
                _ => format!(
                    "{{ if ({name}).is_null() {{ return Err(bindy::Error::Custom(\
                        format!(\"{name} must not be null\"))); }} \
                        (*(({name}) as *const Vec<_>)).clone() }}"
                ),
            },
        }
    }

    fn rust_to_output(&self, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => {
                "*out = if result { 1 } else { 0 };".to_string()
            }
            FfiKind::Prim(_) => "*out = result;".to_string(),
            FfiKind::Bytes => {
                "let result: Vec<u8> = bindy::FromRust::<_, _, bindy::Go>::from_rust(result, &bindy::GoContext)?;\n\
                 let len = result.len();\n\
                 let boxed = result.into_boxed_slice();\n\
                 *out_ptr = Box::into_raw(boxed) as *mut u8;\n\
                 *out_len = len;"
                    .to_string()
            }
            FfiKind::Str => {
                "let result: String = bindy::FromRust::<_, _, bindy::Go>::from_rust(result, &bindy::GoContext)?;\n\
                 let cstr = std::ffi::CString::new(result).map_err(|e| bindy::Error::Custom(e.to_string()))?;\n\
                 *out = cstr.into_raw();"
                    .to_string()
            }
            FfiKind::BigInt => {
                "let big: num_bigint::BigInt = bindy::FromRust::<_, _, bindy::Go>::from_rust(result, &bindy::GoContext)?;\n\
                 let bytes = big.to_signed_bytes_be();\n\
                 let len = bytes.len();\n\
                 let boxed = bytes.into_boxed_slice();\n\
                 *out_ptr = Box::into_raw(boxed) as *mut u8;\n\
                 *out_len = len;"
                    .to_string()
            }
            FfiKind::Class(cls) | FfiKind::Enum(cls) => {
                let ep = &self.entrypoint;
                format!(
                    "let boxed: Box<{ep}::{cls}> = Box::new(result);\n\
                     *out = Box::into_raw(boxed) as *mut std::ffi::c_void;"
                )
            }
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => {
                    let ep = &self.entrypoint;
                    format!(
                        "match result {{\n\
                         Some(v) => *out = Box::into_raw(Box::new(v)) as *mut std::ffi::c_void,\n\
                         None => *out = std::ptr::null_mut(),\n\
                         }}"
                    )
                    .replace("Box::new(v)", &format!("Box::new(v) as Box<{ep}::{cls}>"))
                }
                FfiKind::Bytes => {
                    "match result {\n\
                     Some(v) => {\n\
                         let v: Vec<u8> = bindy::FromRust::<_, _, bindy::Go>::from_rust(v, &bindy::GoContext)?;\n\
                         let len = v.len();\n\
                         let boxed = v.into_boxed_slice();\n\
                         *out_ptr = Box::into_raw(boxed) as *mut u8;\n\
                         *out_len = len;\n\
                     }\n\
                     None => { *out_ptr = std::ptr::null_mut(); *out_len = 0; }\n\
                     }"
                        .to_string()
                }
                FfiKind::Str => {
                    "match result {\n\
                     Some(v) => {\n\
                         let v: String = bindy::FromRust::<_, _, bindy::Go>::from_rust(v, &bindy::GoContext)?;\n\
                         let cstr = std::ffi::CString::new(v).map_err(|e| bindy::Error::Custom(e.to_string()))?;\n\
                         *out = cstr.into_raw();\n\
                     }\n\
                     None => *out = std::ptr::null_mut(),\n\
                     }"
                        .to_string()
                }
                FfiKind::Bool => {
                    "match result {\n\
                     Some(v) => { *out = if v { 1 } else { 0 }; *out_is_some = 1; }\n\
                     None => { *out = 0; *out_is_some = 0; }\n\
                     }"
                        .to_string()
                }
                FfiKind::Prim(_) => {
                    "match result {\n\
                     Some(v) => { *out = v; *out_is_some = 1; }\n\
                     None => { *out_is_some = 0; }\n\
                     }"
                        .to_string()
                }
                FfiKind::BigInt => {
                    "match result {\n\
                     Some(v) => {\n\
                         let v: Vec<u8> = bindy::FromRust::<_, _, bindy::Go>::from_rust(v, &bindy::GoContext)?;\n\
                         let len = v.len();\n\
                         let boxed = v.into_boxed_slice();\n\
                         *out_ptr = Box::into_raw(boxed) as *mut u8;\n\
                         *out_len = len;\n\
                     }\n\
                     None => { *out_ptr = std::ptr::null_mut(); *out_len = 0; }\n\
                     }"
                        .to_string()
                }
                FfiKind::List(_) => {
                    "match result {\n\
                     Some(v) => *out = Box::into_raw(Box::new(v)) as *mut std::ffi::c_void,\n\
                     None => *out = std::ptr::null_mut(),\n\
                     }"
                        .to_string()
                }
                _ => {
                    "match result {\n\
                     Some(v) => *out = Box::into_raw(Box::new(v)) as *mut std::ffi::c_void,\n\
                     None => *out = std::ptr::null_mut(),\n\
                     }"
                        .to_string()
                }
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => {
                    let ep = &self.entrypoint;
                    format!(
                        "let list: Vec<{ep}::{cls}> = result;\n\
                         let boxed: Box<Vec<{ep}::{cls}>> = Box::new(list);\n\
                         *out = Box::into_raw(boxed) as *mut std::ffi::c_void;"
                    )
                }
                _ => {
                    "*out = Box::into_raw(Box::new(result)) as *mut std::ffi::c_void;".to_string()
                }
            },
        }
    }

    fn write_prelude(&mut self) {
        self.out.push_str(
            r#"// AUTO-GENERATED by go-codegen. DO NOT EDIT.
#![allow(clippy::all, clippy::pedantic, clippy::cargo)]
#![allow(unused_imports, unused_variables, dead_code)]
#![allow(improper_ctypes_definitions, unsafe_op_in_unsafe_fn)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;
use chia_sdk_bindings::*;

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn set_last_error(msg: String) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg));
}

unsafe fn catch<F>(f: F) -> i32
where
    F: FnOnce() -> bindy::Result<()> + std::panic::UnwindSafe,
{
    match std::panic::catch_unwind(f) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            set_last_error(e.to_string());
            -1
        }
        Err(_) => {
            set_last_error("panic in FFI call".to_string());
            -1
        }
    }
}

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("failed to create tokio runtime")
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_last_error_length() -> i32 {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(0, |s| i32::try_from(s.len()).unwrap_or(i32::MAX)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_last_error_message(buf: *mut c_char, buf_len: i32) -> i32 {
    if buf.is_null() || buf_len <= 0 {
        return 0;
    }
    LAST_ERROR.with(|e| {
        if let Some(err) = e.borrow().as_ref() {
            let bytes = err.as_bytes();
            let copy_len = bytes.len().min((buf_len as usize).saturating_sub(1));
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
            *(buf.add(copy_len) as *mut u8) = 0;
            i32::try_from(copy_len).unwrap_or(i32::MAX)
        } else {
            0
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_free_bytes(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Box::from_raw(std::slice::from_raw_parts_mut(ptr, len)));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

"#,
        );
    }

    fn write_class(
        &mut self,
        name: &str,
        new: bool,
        fields: &IndexMap<String, String>,
        methods: &IndexMap<String, Method>,
        remote: bool,
    ) {
        let snake = name.to_case(Case::Snake);
        let ep = &self.entrypoint.clone();

        // Free
        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_free(ptr: *mut c_void) {{
    if !ptr.is_null() {{
        drop(Box::from_raw(ptr as *mut {ep}::{name}));
    }}
}}

"#
        ));

        // Clone
        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_clone(ptr: *const c_void, out: *mut *mut c_void) -> i32 {{
    catch(|| {{
        let obj = &*(ptr as *const {ep}::{name});
        *out = Box::into_raw(Box::new(obj.clone())) as *mut c_void;
        Ok(())
    }})
}}

"#
        ));

        // Constructor from fields
        if new {
            let mut params = Vec::new();
            let mut field_inits = Vec::new();

            for (fname, ftype) in fields {
                let kind = self.classify(ftype);
                let pdecl = self.c_param_decl(fname, &kind);
                if !pdecl.is_empty() {
                    params.push(pdecl);
                }
                let conv = self.param_to_rust(fname, &kind, ftype);
                field_inits.push(format!("            {fname}: {conv}"));
            }

            let params_str = if params.is_empty() {
                String::new()
            } else {
                format!("\n    {},", params.join(",\n    "))
            };

            self.out.push_str(&format!(
                r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_new({params_str}
    out: *mut *mut c_void,
) -> i32 {{
    catch(|| {{
        let inner = {ep}::{name} {{
{field_inits}
        }};
        *out = Box::into_raw(Box::new(inner)) as *mut c_void;
        Ok(())
    }})
}}

"#,
                field_inits = field_inits.join(",\n"),
            ));
        }

        // Getters and setters
        for (fname, ftype) in fields {
            let kind = self.classify(ftype);

            // Getter
            let out_decl = self.c_out_decl(&kind);
            let out_params = if out_decl.is_empty() {
                String::new()
            } else {
                format!(", {out_decl}")
            };

            let write_out = self.rust_to_output(&kind).replace("result", "val");
            self.out.push_str(&format!(
                r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_get_{field_snake}(ptr: *const c_void{out_params}) -> i32 {{
    catch(|| {{
        let obj = &*(ptr as *const {ep}::{name});
        let val = obj.{fname}.clone();
        {write_out}
        Ok(())
    }})
}}

"#,
                field_snake = fname.to_case(Case::Snake),
            ));

            // Setter
            let param_decl = self.c_param_decl("value", &kind);
            let set_params = if param_decl.is_empty() {
                String::new()
            } else {
                format!(", {param_decl}")
            };
            let conv = self.param_to_rust("value", &kind, ftype);

            self.out.push_str(&format!(
                r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_set_{field_snake}(ptr: *mut c_void{set_params}) -> i32 {{
    catch(|| {{
        let obj = &mut *(ptr as *mut {ep}::{name});
        obj.{fname} = {conv};
        Ok(())
    }})
}}

"#,
                field_snake = fname.to_case(Case::Snake),
            ));
        }

        // Methods
        for (mname, method) in methods {
            if method.stub_only {
                continue;
            }

            let method_snake = mname.to_case(Case::Snake);
            let is_async =
                method.kind == MethodKind::Async || method.kind == MethodKind::AsyncFactory;
            let is_instance = matches!(
                method.kind,
                MethodKind::Normal | MethodKind::Async | MethodKind::ToString
            );
            let is_factory = matches!(
                method.kind,
                MethodKind::Factory | MethodKind::AsyncFactory
            );
            let is_constructor = method.kind == MethodKind::Constructor;

            // Determine return type
            let ret_type_str = method.ret.as_deref().unwrap_or(
                if is_factory || is_constructor {
                    "Self"
                } else {
                    "()"
                },
            );

            let ret_kind = if ret_type_str == "Self" {
                FfiKind::Class(name.to_string())
            } else {
                self.classify(ret_type_str)
            };

            // Build parameters
            let mut params = Vec::new();
            if is_instance {
                params.push("ptr: *const c_void".to_string());
            }
            for (aname, atype) in &method.args {
                let akind = self.classify(atype);
                let pdecl = self.c_param_decl(aname, &akind);
                if !pdecl.is_empty() {
                    params.push(pdecl);
                }
            }
            let out_decl = self.c_out_decl(&ret_kind);
            if !out_decl.is_empty() {
                params.push(out_decl);
            }

            let params_str = if params.is_empty() {
                String::new()
            } else {
                params.join(", ")
            };

            // Build argument conversions
            let mut arg_convs = Vec::new();
            for (aname, atype) in &method.args {
                let akind = self.classify(atype);
                arg_convs.push(self.param_to_rust(aname, &akind, atype));
            }
            let args_str = arg_convs.join(",\n            ");

            // Build the call expression
            let fully_qualified = if remote {
                format!(
                    "<{ep}::{name} as {ep}::{name}Ext>",
                )
            } else {
                format!("{ep}::{name}")
            };

            let call = if is_instance {
                let self_arg = if method.kind == MethodKind::Async {
                    // Async instance methods use obj.method() pattern
                    format!(
                        "let obj = &*(ptr as *const {ep}::{name});\n        \
                         let result = runtime().block_on(obj.{mname}({args_str}))?;"
                    )
                } else {
                    let args_with_self = if args_str.is_empty() {
                        format!("&*(ptr as *const {ep}::{name})")
                    } else {
                        format!(
                            "&*(ptr as *const {ep}::{name}),\n            {args_str}"
                        )
                    };
                    format!("let result = {fully_qualified}::{mname}({args_with_self})?;")
                };
                self_arg
            } else if is_async {
                format!(
                    "let result = runtime().block_on({fully_qualified}::{mname}({args_str}))?;"
                )
            } else if is_constructor && new {
                // Constructor is handled by `new` above
                continue;
            } else {
                format!("let result = {fully_qualified}::{mname}({args_str})?;")
            };

            let write_out = self.rust_to_output(&ret_kind);

            self.out.push_str(&format!(
                r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_{method_snake}({params_str}) -> i32 {{
    catch(|| {{
        {call}
        {write_out}
        Ok(())
    }})
}}

"#
            ));
        }

        // List helpers for this class
        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_list_len(ptr: *const c_void) -> usize {{
    if ptr.is_null() {{
        return 0;
    }}
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{
        let list = &*(ptr as *const Vec<{ep}::{name}>);
        list.len()
    }})).unwrap_or(0)
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_list_get(ptr: *const c_void, index: usize, out: *mut *mut c_void) -> i32 {{
    catch(|| {{
        if ptr.is_null() {{
            return Err(bindy::Error::Custom("null pointer".to_string()));
        }}
        let list = &*(ptr as *const Vec<{ep}::{name}>);
        if index >= list.len() {{
            return Err(bindy::Error::Custom("index out of bounds".to_string()));
        }}
        *out = Box::into_raw(Box::new(list[index].clone())) as *mut c_void;
        Ok(())
    }})
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_list_free(ptr: *mut c_void) {{
    if !ptr.is_null() {{
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{
            drop(Box::from_raw(ptr as *mut Vec<{ep}::{name}>));
        }}));
    }}
}}

"#
        ));
    }

    fn write_enum(&mut self, name: &str, values: &[String]) {
        let snake = name.to_case(Case::Snake);
        let ep = &self.entrypoint.clone();

        // Enum → i32 conversion
        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_to_int(ptr: *const c_void, out: *mut i32) -> i32 {{
    catch(|| {{
        let val = &*(ptr as *const {ep}::{name});
        *out = match val {{
"#
        ));
        for (i, v) in values.iter().enumerate() {
            self.out
                .push_str(&format!("            {ep}::{name}::{v} => {i},\n"));
        }
        self.out.push_str(
            r#"        };
        Ok(())
    })
}

"#,
        );

        // i32 → Enum conversion
        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_from_int(value: i32, out: *mut *mut c_void) -> i32 {{
    catch(|| {{
        let val = match value {{
"#
        ));
        for (i, v) in values.iter().enumerate() {
            self.out
                .push_str(&format!("            {i} => {ep}::{name}::{v},\n"));
        }
        self.out.push_str(&format!(
            r#"            _ => return Err(bindy::Error::Custom(format!("invalid {name} value: {{value}}"))),
        }};
        *out = Box::into_raw(Box::new(val)) as *mut c_void;
        Ok(())
    }})
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}_free(ptr: *mut c_void) {{
    if !ptr.is_null() {{
        drop(Box::from_raw(ptr as *mut {ep}::{name}));
    }}
}}

"#
        ));
    }

    fn write_function(&mut self, name: &str, args: &IndexMap<String, String>, ret: &Option<String>) {
        let snake = name.to_case(Case::Snake);
        let ep = &self.entrypoint.clone();

        let ret_type_str = ret.as_deref().unwrap_or("()");
        let ret_kind = self.classify(ret_type_str);

        let mut params = Vec::new();
        for (aname, atype) in args {
            let akind = self.classify(atype);
            let pdecl = self.c_param_decl(aname, &akind);
            if !pdecl.is_empty() {
                params.push(pdecl);
            }
        }
        let out_decl = self.c_out_decl(&ret_kind);
        if !out_decl.is_empty() {
            params.push(out_decl);
        }

        let params_str = params.join(", ");

        let mut arg_convs = Vec::new();
        for (aname, atype) in args {
            let akind = self.classify(atype);
            arg_convs.push(self.param_to_rust(aname, &akind, atype));
        }
        let args_str = arg_convs.join(",\n            ");

        let write_out = self.rust_to_output(&ret_kind);

        self.out.push_str(&format!(
            r#"#[unsafe(no_mangle)]
pub unsafe extern "C" fn go_{snake}({params_str}) -> i32 {{
    catch(|| {{
        let result = {ep}::{name}({args_str})?;
        {write_out}
        Ok(())
    }})
}}

"#
        ));
    }
}

// ── Go code generation ─────────────────────────────────────────────────────

struct GoGen {
    externs: String,
    body: String,
    c_func_names: Vec<String>,
    mappings: IndexMap<String, String>,
    classes: HashSet<String>,
    enums: HashSet<String>,
}

impl GoGen {
    fn new(mappings: IndexMap<String, String>, classes: HashSet<String>, enums: HashSet<String>) -> Self {
        Self {
            externs: String::new(),
            body: String::new(),
            c_func_names: Vec::new(),
            mappings,
            classes,
            enums,
        }
    }

    fn classify(&self, ty: &str) -> FfiKind {
        classify(ty, &self.mappings, &self.classes, &self.enums)
    }

    /// Convert a PascalCase name into a lowercase spaced form for use in doc comments.
    /// e.g. "AddCoinSpend" → "add coin spend", "P2PuzzleHashes" → "P2 puzzle hashes".
    fn humanize(pascal: &str) -> String {
        let mut words = Vec::new();
        let mut cur = String::new();
        for ch in pascal.chars() {
            if ch.is_ascii_uppercase() && !cur.is_empty() {
                // Don't split consecutive uppercase (e.g. "P2" stays together)
                if cur.chars().last().map_or(false, |c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                    words.push(cur);
                    cur = String::new();
                }
            }
            cur.push(ch);
        }
        if !cur.is_empty() {
            words.push(cur);
        }
        if words.is_empty() {
            return pascal.to_string();
        }
        // Lowercase all words except those that look like acronyms/identifiers (all-uppercase + digits)
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let is_acronym = w.len() <= 3
                    && w.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
                if is_acronym && i > 0 {
                    w.to_string()
                } else if i == 0 {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(first) => {
                            first.to_lowercase().to_string() + &chars.as_str().to_string()
                        }
                        None => String::new(),
                    }
                } else {
                    w.to_lowercase()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Generate a smart fallback doc comment for a method based on its signature.
    fn generate_method_doc(
        class_name: &str,
        method_name: &str,     // original snake_case
        go_name: &str,         // PascalCase Go name
        method: &Method,
        is_instance: bool,
    ) -> String {
        let has_return = method.ret.is_some()
            || matches!(
                method.kind,
                MethodKind::Factory | MethodKind::AsyncFactory | MethodKind::Constructor
            );
        let is_bool_return = method
            .ret
            .as_deref()
            .map_or(false, |r| r == "bool");
        let has_args = !method.args.is_empty();

        // 0. "new" constructors (static kind but named "new")
        if method_name == "new" {
            if has_args {
                let arg_names: Vec<String> = method
                    .args
                    .keys()
                    .map(|a| a.to_case(Case::Camel))
                    .collect();
                return format!(
                    "creates a new [{class_name}] with the given {}.",
                    arg_names.join(", ")
                );
            }
            return format!("creates a new [{class_name}].");
        }

        // 1. Boolean predicates: is_*, has_*
        if is_bool_return && (method_name.starts_with("is_") || method_name.starts_with("has_")) {
            let predicate = if method_name.starts_with("is_") {
                &method_name[3..]
            } else {
                &method_name[4..]
            };
            let human = Self::humanize(&predicate.to_case(Case::Pascal));
            return format!("reports whether the [{class_name}] is {human}.");
        }

        // 2. to_* converters (instance, has return, no extra args)
        if is_instance && has_return && !has_args && method_name.starts_with("to_") {
            let target = &method_name[3..];
            let human = Self::humanize(&target.to_case(Case::Pascal));
            return format!("returns the {human} representation of the [{class_name}].");
        }

        // 3. Getters (instance, no args, has return)
        if is_instance && has_return && !has_args {
            let human = Self::humanize(go_name);
            return format!("returns the {human} of the [{class_name}].");
        }

        // 4. Instance actions with args
        if is_instance && has_args {
            let human = Self::humanize(go_name);
            let arg_names: Vec<String> = method
                .args
                .keys()
                .map(|a| a.to_case(Case::Camel))
                .collect();
            if has_return {
                return format!(
                    "computes the {human} for the given {}.",
                    arg_names.join(", ")
                );
            }
            return format!(
                "performs {human} with the given {}.",
                arg_names.join(", ")
            );
        }

        // 5. Static / factory methods with args
        if !is_instance && has_args {
            let human = Self::humanize(go_name);
            let arg_names: Vec<String> = method
                .args
                .keys()
                .map(|a| a.to_case(Case::Camel))
                .collect();
            if has_return {
                return format!(
                    "computes the {human} for the given {}.",
                    arg_names.join(", ")
                );
            }
            return format!(
                "performs {human} with the given {}.",
                arg_names.join(", ")
            );
        }

        // 6. Static, no args, has return
        if !is_instance && has_return && !has_args {
            let human = Self::humanize(go_name);
            return format!("returns the {human}.");
        }

        // 7. Fallback
        let human = Self::humanize(go_name);
        format!("performs {human} on the [{class_name}].")
    }

    /// Generate a smart fallback doc comment for a standalone function.
    fn generate_function_doc(
        go_name: &str,
        args: &IndexMap<String, String>,
        ret: &Option<String>,
    ) -> String {
        let has_return = ret.is_some();
        let has_args = !args.is_empty();

        if has_args {
            let arg_names: Vec<String> = args
                .keys()
                .map(|a| a.to_case(Case::Camel))
                .collect();
            let human = Self::humanize(go_name);
            if has_return {
                return format!(
                    "computes the {human} for the given {}.",
                    arg_names.join(", ")
                );
            }
            return format!(
                "performs {human} with the given {}.",
                arg_names.join(", ")
            );
        }

        let human = Self::humanize(go_name);
        if has_return {
            format!("returns the {human}.")
        } else {
            format!("performs {human}.")
        }
    }

    /// Rename a PascalCase Go method name to idiomatic Go conventions.
    /// Only applies to instance converter methods (To* with no extra args).
    fn go_rename_method(pascal: &str, is_instance: bool, has_args: bool) -> String {
        if !is_instance || has_args {
            return pascal.to_string();
        }
        if let Some(rest) = pascal.strip_prefix("To") {
            if !rest.is_empty() && rest.chars().next().unwrap().is_ascii_uppercase() {
                return rest.to_string();
            }
        }
        pascal.to_string()
    }

    fn go_type(&self, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => "bool".to_string(),
            FfiKind::Prim(t) => match t.as_str() {
                "u8" => "uint8".to_string(),
                "i8" => "int8".to_string(),
                "u16" => "uint16".to_string(),
                "i16" => "int16".to_string(),
                "u32" => "uint32".to_string(),
                "i32" => "int32".to_string(),
                "u64" => "uint64".to_string(),
                "i64" => "int64".to_string(),
                "usize" => "uint".to_string(),
                "f32" => "float32".to_string(),
                "f64" => "float64".to_string(),
                _ => "uint64".to_string(),
            },
            FfiKind::Bytes => "[]byte".to_string(),
            FfiKind::Str => "string".to_string(),
            FfiKind::BigInt => "*big.Int".to_string(),
            FfiKind::Class(cls) | FfiKind::Enum(cls) => format!("*{cls}"),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Bool => "*bool".to_string(),
                FfiKind::Prim(t) => format!("*{}", self.go_type(&FfiKind::Prim(t.clone()))),
                FfiKind::Bytes => "[]byte".to_string(),
                FfiKind::Str => "*string".to_string(),
                FfiKind::Class(cls) | FfiKind::Enum(cls) => format!("*{cls}"),
                FfiKind::BigInt => "*big.Int".to_string(),
                _ => "unsafe.Pointer".to_string(),
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => format!("[]*{cls}"),
                _ => "unsafe.Pointer".to_string(),
            },
        }
    }

    fn write_prelude(&mut self) {
        self.externs.push_str(
            r#"// Error handling
extern int go_last_error_length();
extern int go_last_error_message(char* buf, int buf_len);
extern void go_free_bytes(uint8_t* ptr, size_t len);
extern void go_free_string(char* ptr);
"#,
        );
        self.c_func_names.extend([
            "go_last_error_length".to_string(),
            "go_last_error_message".to_string(),
            "go_free_bytes".to_string(),
            "go_free_string".to_string(),
        ]);

        self.body.push_str(
            r#"
func lastError() error {
	length := C.go_last_error_length()
	if length == 0 {
		return fmt.Errorf("unknown error")
	}
	buf := make([]byte, length+1)
	C.go_last_error_message((*C.char)(unsafe.Pointer(&buf[0])), C.int(length+1))
	return fmt.Errorf("%s", string(buf[:length]))
}

func bytesToPtr(b []byte) (*C.uint8_t, C.size_t) {
	if len(b) == 0 {
		return nil, 0
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0])), C.size_t(len(b))
}

func bigIntToSignedBytes(n *big.Int) []byte {
	if n.Sign() == 0 {
		return []byte{0}
	}
	if n.Sign() > 0 {
		b := n.Bytes()
		if len(b) > 0 && b[0]&0x80 != 0 {
			b = append([]byte{0}, b...)
		}
		return b
	}
	bitLen := n.BitLen() + 1
	byteLen := (bitLen + 7) / 8
	mod := new(big.Int).Lsh(big.NewInt(1), uint(byteLen*8))
	b := new(big.Int).Add(n, mod).Bytes()
	for len(b) < byteLen {
		b = append([]byte{0xff}, b...)
	}
	return b
}

func bigIntFromSignedBytes(b []byte) *big.Int {
	if len(b) == 0 {
		return new(big.Int)
	}
	n := new(big.Int).SetBytes(b)
	if b[0]&0x80 != 0 {
		mod := new(big.Int).Lsh(big.NewInt(1), uint(len(b)*8))
		n.Sub(n, mod)
	}
	return n
}

"#,
        );
    }

    fn write_class(
        &mut self,
        name: &str,
        doc: &Option<String>,
        new: bool,
        fields: &IndexMap<String, String>,
        methods: &IndexMap<String, Method>,
        _remote: bool,
    ) {
        let snake = name.to_case(Case::Snake);

        // C extern declarations
        self.externs
            .push_str(&format!("\n// {name}\n"));
        self.externs.push_str(&format!(
            "extern void go_{snake}_free(void* ptr);\n"
        ));
        self.externs.push_str(&format!(
            "extern int go_{snake}_clone(const void* ptr, void** out);\n"
        ));
        self.c_func_names.push(format!("go_{snake}_free"));
        self.c_func_names.push(format!("go_{snake}_clone"));

        // Go struct
        let type_doc = match doc {
            Some(d) => format!("// {name} {d}"),
            None => format!("// {name} represents a {name} in the Chia wallet SDK."),
        };
        self.body.push_str(&format!(
            r#"{type_doc}
//
// All methods are safe for concurrent use from multiple goroutines.
// Call [Free] or [Close] to release the underlying memory, or rely on the
// attached runtime finalizer for automatic cleanup.
type {name} struct {{
	ptr unsafe.Pointer
	mu  sync.RWMutex
}}

// Free releases the underlying Rust object.
// Calling Free more than once or on a nil receiver is safe and has no effect.
// Free blocks until any concurrent method calls complete.
func (o *{name}) Free() {{
	if o == nil {{
		return
	}}
	o.mu.Lock()
	defer o.mu.Unlock()
	if o.ptr == nil {{
		return
	}}
	C.go_{snake}_free(o.ptr)
	o.ptr = nil
}}

// Close implements [io.Closer] by releasing the underlying resource.
// It is safe to call Close multiple times and concurrently with other methods.
func (o *{name}) Close() error {{
	o.Free()
	return nil
}}

// Clone returns a deep copy of the {name}.
func (o *{name}) Clone() (*{name}, error) {{
	if o == nil {{
		return nil, fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return nil, fmt.Errorf("object is nil or already freed")
	}}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	var out unsafe.Pointer
	ret := C.go_{snake}_clone(o.ptr, &out)
	if ret != 0 {{
		return nil, lastError()
	}}
	clone := &{name}{{ptr: out}}
	runtime.SetFinalizer(clone, (*{name}).Free)
	return clone, nil
}}

"#
        ));

        // Constructor
        if new {
            self.write_constructor(name, fields);
        }

        // Getters and setters
        for (fname, ftype) in fields {
            self.write_getter(name, fname, ftype);
            self.write_setter(name, fname, ftype);
        }

        // Methods
        for (mname, method) in methods {
            if method.stub_only {
                continue;
            }
            if method.kind == MethodKind::Constructor && new {
                continue;
            }
            self.write_method(name, mname, method);
        }

        // List helpers
        self.externs.push_str(&format!(
            "extern size_t go_{snake}_list_len(const void* ptr);\n"
        ));
        self.externs.push_str(&format!(
            "extern int go_{snake}_list_get(const void* ptr, size_t index, void** out);\n"
        ));
        self.externs.push_str(&format!(
            "extern void go_{snake}_list_free(void* ptr);\n"
        ));
        self.c_func_names.push(format!("go_{snake}_list_len"));
        self.c_func_names.push(format!("go_{snake}_list_get"));
        self.c_func_names.push(format!("go_{snake}_list_free"));
    }

    fn write_constructor(&mut self, name: &str, fields: &IndexMap<String, String>) {
        let snake = name.to_case(Case::Snake);

        // Extern declaration
        let mut c_params = Vec::new();
        for (fname, ftype) in fields {
            let kind = self.classify(ftype);
            c_params.push(self.c_extern_param(fname, &kind));
        }
        c_params.push("void** out".to_string());
        self.externs.push_str(&format!(
            "extern int go_{snake}_new({});\n",
            c_params.join(", ")
        ));
        self.c_func_names.push(format!("go_{snake}_new"));

        // Go function
        let mut go_params = Vec::new();
        let mut call_args = Vec::new();
        let mut pre_call = Vec::new();
        let mut post_call = Vec::new();

        for (fname, ftype) in fields {
            let kind = self.classify(ftype);
            let go_name = fname.to_case(Case::Camel);
            let go_ty = self.go_type(&kind);
            go_params.push(format!("{go_name} {go_ty}"));
            let (pre, args, post) = self.go_to_c_param(&go_name, &kind);
            pre_call.extend(pre);
            call_args.extend(args);
            post_call.extend(post);
        }

        let pre_str = pre_call.join("\n\t");
        let post_str = post_call.join("\n\t");
        let go_params_str = go_params.join(", ");

        let call_args_str = call_args.join(", ");
        let c_call_args = if call_args_str.is_empty() {
            "&out".to_string()
        } else {
            format!("{call_args_str}, &out")
        };

        // Collect nil checks for binding-type fields and KeepAlive calls
        let mut nil_check_lines = Vec::new();
        let mut keep_alive_lines = Vec::new();

        for (fname, ftype) in fields {
            let fkind = self.classify(ftype);
            let go_name = fname.to_case(Case::Camel);
            match &fkind {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    nil_check_lines.push(format!(
                        "if {go_name} == nil {{\n\t\treturn nil, fmt.Errorf(\"{go_name} must not be nil\")\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::List(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    nil_check_lines.push(format!(
                        "for i, item := range {go_name} {{\n\t\tif item == nil {{\n\t\t\treturn nil, fmt.Errorf(\"nil item in {go_name} at index %d\", i)\n\t\t}}\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::Opt(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                _ => {}
            }
        }

        let nil_checks_str = if nil_check_lines.is_empty() {
            String::new()
        } else {
            nil_check_lines.join("\n\t") + "\n\t"
        };

        let keep_alive_str = if keep_alive_lines.is_empty() {
            String::new()
        } else {
            "\n\t".to_string() + &keep_alive_lines.join("\n\t")
        };

        self.body.push_str(&format!(
            r#"// New{name} creates a new [{name}] with the given field values.
func New{name}({go_params_str}) (*{name}, error) {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{nil_checks_str}{pre_str}
	{post_str}
	var out unsafe.Pointer
	ret := C.go_{snake}_new({c_call_args}){keep_alive_str}
	if ret != 0 {{
		return nil, lastError()
	}}
	obj := &{name}{{ptr: out}}
	runtime.SetFinalizer(obj, (*{name}).Free)
	return obj, nil
}}

"#,
        ));
    }

    fn write_getter(&mut self, class_name: &str, fname: &str, ftype: &str) {
        let snake = class_name.to_case(Case::Snake);
        let field_snake = fname.to_case(Case::Snake);
        let kind = self.classify(ftype);
        let go_name = fname.to_case(Case::Pascal);
        let go_ty = self.go_type(&kind);

        if go_ty.is_empty() {
            return;
        }

        // Extern
        let mut c_params = vec![format!("const void* ptr")];
        c_params.extend(self.c_extern_out(&kind));
        self.externs.push_str(&format!(
            "extern int go_{snake}_get_{field_snake}({});\n",
            c_params.join(", ")
        ));
        self.c_func_names.push(format!("go_{snake}_get_{field_snake}"));

        // Go getter
        let (out_decl, call_out, result_expr) = self.c_to_go_output(&kind);

        self.body.push_str(&format!(
            r#"// {go_name} returns the {go_name} field of the [{class_name}].
func (o *{class_name}) {go_name}() ({go_ty}, error) {{
	if o == nil {{
		return {zero}, fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return {zero}, fmt.Errorf("object is nil or already freed")
	}}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{out_decl}
	ret := C.go_{snake}_get_{field_snake}(o.ptr, {call_out})
	runtime.KeepAlive(o)
	if ret != 0 {{
		return {zero}, lastError()
	}}
	{result_expr}
}}

"#,
            zero = self.go_zero(&kind),
        ));
    }

    fn write_setter(&mut self, class_name: &str, fname: &str, ftype: &str) {
        let snake = class_name.to_case(Case::Snake);
        let field_snake = fname.to_case(Case::Snake);
        let kind = self.classify(ftype);
        let field_pascal = fname.to_case(Case::Pascal);
        let go_name = format!("Set{field_pascal}");

        // Extern
        let mut c_params = vec!["void* ptr".to_string()];
        c_params.push(self.c_extern_param("value", &kind));
        self.externs.push_str(&format!(
            "extern int go_{snake}_set_{field_snake}({});\n",
            c_params.join(", ")
        ));
        self.c_func_names.push(format!("go_{snake}_set_{field_snake}"));

        // Go setter
        let go_ty = self.go_type(&kind);
        let (pre, args, post) = self.go_to_c_param("value", &kind);
        let pre_str = pre.join("\n\t");
        let post_str = post.join("\n\t");

        // Nil check and KeepAlive for binding-type value
        let value_nil_check = match &kind {
            FfiKind::Class(_) | FfiKind::Enum(_) => {
                "if value == nil {\n\t\treturn fmt.Errorf(\"value must not be nil\")\n\t}\n\t".to_string()
            }
            _ => String::new(),
        };

        let mut keep_alive_lines = vec!["runtime.KeepAlive(o)".to_string()];
        if matches!(&kind, FfiKind::Class(_) | FfiKind::Enum(_)) {
            keep_alive_lines.push("runtime.KeepAlive(value)".to_string());
        }
        let keep_alive_str = "\n\t".to_string() + &keep_alive_lines.join("\n\t");

        self.body.push_str(&format!(
            r#"// {go_name} updates the {field_pascal} field of the [{class_name}].
func (o *{class_name}) {go_name}(value {go_ty}) error {{
	if o == nil {{
		return fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return fmt.Errorf("object is nil or already freed")
	}}
	{value_nil_check}runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{pre_str}
	{post_str}
	ret := C.go_{snake}_set_{field_snake}(o.ptr, {args}){keep_alive_str}
	if ret != 0 {{
		return lastError()
	}}
	return nil
}}

"#,
            args = args.join(", "),
        ));
    }

    fn write_method(&mut self, class_name: &str, mname: &str, method: &Method) {
        let snake = class_name.to_case(Case::Snake);
        let method_snake = mname.to_case(Case::Snake);
        let is_instance = matches!(
            method.kind,
            MethodKind::Normal | MethodKind::Async | MethodKind::ToString
        );
        let is_factory = matches!(
            method.kind,
            MethodKind::Factory | MethodKind::AsyncFactory
        );
        let go_method_name = Self::go_rename_method(
            &mname.to_case(Case::Pascal),
            is_instance,
            !method.args.is_empty(),
        );

        let ret_type_str = method.ret.as_deref().unwrap_or(
            if is_factory || method.kind == MethodKind::Constructor {
                "Self"
            } else {
                "()"
            },
        );

        let ret_kind = if ret_type_str == "Self" {
            FfiKind::Class(class_name.to_string())
        } else {
            self.classify(ret_type_str)
        };

        // C extern declaration
        let mut c_params = Vec::new();
        if is_instance {
            c_params.push("const void* ptr".to_string());
        }
        for (aname, atype) in &method.args {
            let akind = self.classify(atype);
            c_params.push(self.c_extern_param(aname, &akind));
        }
        c_params.extend(self.c_extern_out(&ret_kind));
        self.externs.push_str(&format!(
            "extern int go_{snake}_{method_snake}({});\n",
            c_params.join(", ")
        ));
        self.c_func_names.push(format!("go_{snake}_{method_snake}"));

        // Go function
        let mut go_params = Vec::new();
        let mut pre_call = Vec::new();
        let mut call_args = Vec::new();
        let mut post_call = Vec::new();

        if is_instance {
            call_args.push("o.ptr".to_string());
        }

        for (aname, atype) in &method.args {
            let akind = self.classify(atype);
            let go_name = aname.to_case(Case::Camel);
            let go_ty = self.go_type(&akind);
            go_params.push(format!("{go_name} {go_ty}"));
            let (pre, args, post) = self.go_to_c_param(&go_name, &akind);
            pre_call.extend(pre);
            call_args.extend(args);
            post_call.extend(post);
        }

        let go_ret_ty = self.go_type(&ret_kind);
        let is_void = matches!(ret_kind, FfiKind::Void);

        let (out_decl, call_out, result_expr) = self.c_to_go_output(&ret_kind);
        if !is_void {
            call_args.push(call_out.clone());
        }

        let pre_str = pre_call.join("\n\t");
        let post_str = post_call.join("\n\t");
        let go_params_str = go_params.join(", ");
        let call_args_str = call_args.join(", ");

        // Collect nil checks for binding-type args and KeepAlive calls
        let err_return_prefix = if matches!(ret_kind, FfiKind::Void) {
            "return".to_string()
        } else {
            format!("return {},", self.go_zero(&ret_kind))
        };

        let mut nil_check_lines = Vec::new();
        let mut keep_alive_lines = Vec::new();

        if is_instance {
            keep_alive_lines.push("runtime.KeepAlive(o)".to_string());
        }

        for (aname, atype) in &method.args {
            let akind = self.classify(atype);
            let go_name = aname.to_case(Case::Camel);
            match &akind {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    nil_check_lines.push(format!(
                        "if {go_name} == nil {{\n\t\t{err_return_prefix} fmt.Errorf(\"{go_name} must not be nil\")\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::List(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    nil_check_lines.push(format!(
                        "for i, item := range {go_name} {{\n\t\tif item == nil {{\n\t\t\t{err_return_prefix} fmt.Errorf(\"nil item in {go_name} at index %d\", i)\n\t\t}}\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::Opt(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                _ => {}
            }
        }

        let nil_checks_str = if nil_check_lines.is_empty() {
            String::new()
        } else {
            nil_check_lines.join("\n\t") + "\n\t"
        };

        let keep_alive_str = if keep_alive_lines.is_empty() {
            String::new()
        } else {
            "\n\t".to_string() + &keep_alive_lines.join("\n\t")
        };

        // Determine if this is a method or function
        let method_doc = match &method.doc {
            Some(d) => format!("// {go_method_name} {d}"),
            None => {
                let doc = Self::generate_method_doc(class_name, mname, &go_method_name, method, is_instance);
                format!("// {go_method_name} {doc}")
            }
        };

        if is_instance {
            if is_void {
                self.body.push_str(&format!(
                    r#"{method_doc}
func (o *{class_name}) {go_method_name}({go_params_str}) error {{
	if o == nil {{
		return fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return fmt.Errorf("object is nil or already freed")
	}}
	{nil_checks_str}runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{pre_str}
	{post_str}
	ret := C.go_{snake}_{method_snake}({call_args_str}){keep_alive_str}
	if ret != 0 {{
		return lastError()
	}}
	return nil
}}

"#
                ));
            } else {
                self.body.push_str(&format!(
                    r#"{method_doc}
func (o *{class_name}) {go_method_name}({go_params_str}) ({go_ret_ty}, error) {{
	if o == nil {{
		return {zero}, fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return {zero}, fmt.Errorf("object is nil or already freed")
	}}
	{nil_checks_str}runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{pre_str}
	{post_str}
	{out_decl}
	ret := C.go_{snake}_{method_snake}({call_args_str}){keep_alive_str}
	if ret != 0 {{
		return {zero}, lastError()
	}}
	{result_expr}
}}

"#,
                    zero = self.go_zero(&ret_kind),
                ));
            }
        } else {
            // Static / Factory - generate as package-level function
            let func_name = if is_factory {
                format!("New{class_name}{go_method_name}")
            } else {
                format!("{class_name}{go_method_name}")
            };

            if is_void {
                let static_doc = match &method.doc {
                    Some(d) => format!("// {func_name} {d}"),
                    None => {
                        let doc = Self::generate_method_doc(class_name, mname, &go_method_name, method, false);
                        format!("// {func_name} {doc}")
                    }
                };
                self.body.push_str(&format!(
                    r#"{static_doc}
func {func_name}({go_params_str}) error {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{nil_checks_str}{pre_str}
	{post_str}
	ret := C.go_{snake}_{method_snake}({call_args_str}){keep_alive_str}
	if ret != 0 {{
		return lastError()
	}}
	return nil
}}

"#
                ));
            } else {
                let doc = match &method.doc {
                    Some(d) => format!("// {func_name} {d}"),
                    None => if is_factory {
                        format!("// {func_name} creates a new [{class_name}] via the {go_method_name} factory.")
                    } else {
                        let doc = Self::generate_method_doc(class_name, mname, &go_method_name, method, false);
                        format!("// {func_name} {doc}")
                    },
                };
                self.body.push_str(&format!(
                    r#"{doc}
func {func_name}({go_params_str}) ({go_ret_ty}, error) {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{nil_checks_str}{pre_str}
	{post_str}
	{out_decl}
	ret := C.go_{snake}_{method_snake}({call_args_str}){keep_alive_str}
	if ret != 0 {{
		return {zero}, lastError()
	}}
	{result_expr}
}}

"#,
                    zero = self.go_zero(&ret_kind),
                ));
            }
        }

        // If return is a class, set finalizer in result_expr
    }

    fn write_enum(&mut self, name: &str, doc: &Option<String>, values: &[String]) {
        let snake = name.to_case(Case::Snake);

        self.externs.push_str(&format!("\n// {name}\n"));
        self.externs.push_str(&format!(
            "extern int go_{snake}_to_int(const void* ptr, int* out);\n"
        ));
        self.externs.push_str(&format!(
            "extern int go_{snake}_from_int(int value, void** out);\n"
        ));
        self.externs.push_str(&format!(
            "extern void go_{snake}_free(void* ptr);\n"
        ));
        self.c_func_names.push(format!("go_{snake}_to_int"));
        self.c_func_names.push(format!("go_{snake}_from_int"));
        self.c_func_names.push(format!("go_{snake}_free"));

        // Go enum as struct with ptr (like classes, since Rust uses opaque pointers)
        let enum_doc = match doc {
            Some(d) => format!("// {name} {d}"),
            None => format!("// {name} represents a {name} variant."),
        };
        self.body.push_str(&format!(
            r#"{enum_doc}
//
// All methods are safe for concurrent use from multiple goroutines.
// Use the New{name}* constructors or [New{name}FromInt] to create instances.
type {name} struct {{
	ptr unsafe.Pointer
	mu  sync.RWMutex
}}

// Free releases the underlying Rust object.
// Calling Free more than once or on a nil receiver is safe and has no effect.
// Free blocks until any concurrent method calls complete.
func (o *{name}) Free() {{
	if o == nil {{
		return
	}}
	o.mu.Lock()
	defer o.mu.Unlock()
	if o.ptr == nil {{
		return
	}}
	C.go_{snake}_free(o.ptr)
	o.ptr = nil
}}

// Close implements [io.Closer] by releasing the underlying resource.
// It is safe to call Close multiple times and concurrently with other methods.
func (o *{name}) Close() error {{
	o.Free()
	return nil
}}

// ToInt returns the integer value of this enum variant.
func (o *{name}) ToInt() (int, error) {{
	if o == nil {{
		return 0, fmt.Errorf("object is nil or already freed")
	}}
	o.mu.RLock()
	defer o.mu.RUnlock()
	if o.ptr == nil {{
		return 0, fmt.Errorf("object is nil or already freed")
	}}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	var out C.int
	ret := C.go_{snake}_to_int(o.ptr, &out)
	if ret != 0 {{
		return 0, lastError()
	}}
	return int(out), nil
}}

"#
        ));

        // Constants for enum values
        let const_names: Vec<String> = values.iter()
            .map(|v| format!("{name}Value{}", v.to_case(Case::Pascal)))
            .collect();
        let max_len = const_names.iter().map(|n| n.len()).max().unwrap_or(0);
        self.body.push_str(&format!("// Integer constants for [{name}] variants.\nconst (\n"));
        for (i, cname) in const_names.iter().enumerate() {
            self.body.push_str(&format!(
                "\t{cname:<width$} = {i}\n", width = max_len
            ));
        }
        self.body.push_str(")\n\n");

        // Factory function: New{Name}FromInt
        self.body.push_str(&format!(
            r#"// New{name}FromInt creates a [{name}] from its integer representation.
func New{name}FromInt(value int) (*{name}, error) {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	var out unsafe.Pointer
	ret := C.go_{snake}_from_int(C.int(value), &out)
	if ret != 0 {{
		return nil, lastError()
	}}
	obj := &{name}{{ptr: out}}
	runtime.SetFinalizer(obj, (*{name}).Free)
	return obj, nil
}}

"#
        ));

        // Convenience factory for each variant
        for (i, v) in values.iter().enumerate() {
            let go_name = v.to_case(Case::Pascal);
            self.body.push_str(&format!(
                r#"// New{name}{go_name} creates the {go_name} variant of [{name}].
func New{name}{go_name}() (*{name}, error) {{
	return New{name}FromInt({i})
}}

"#
            ));
        }
    }

    fn write_function(&mut self, name: &str, doc: &Option<String>, args: &IndexMap<String, String>, ret: &Option<String>) {
        let snake = name.to_case(Case::Snake);
        let go_name = name.to_case(Case::Pascal);

        let ret_type_str = ret.as_deref().unwrap_or("()");
        let ret_kind = self.classify(ret_type_str);

        // Extern
        let mut c_params = Vec::new();
        for (aname, atype) in args {
            let akind = self.classify(atype);
            c_params.push(self.c_extern_param(aname, &akind));
        }
        c_params.extend(self.c_extern_out(&ret_kind));
        self.externs.push_str(&format!(
            "extern int go_{snake}({});\n",
            c_params.join(", ")
        ));
        self.c_func_names.push(format!("go_{snake}"));

        // Go function
        let mut go_params = Vec::new();
        let mut pre_call = Vec::new();
        let mut call_args = Vec::new();
        let mut post_call = Vec::new();

        for (aname, atype) in args {
            let akind = self.classify(atype);
            let go_param_name = aname.to_case(Case::Camel);
            let go_ty = self.go_type(&akind);
            go_params.push(format!("{go_param_name} {go_ty}"));
            let (pre, a, post) = self.go_to_c_param(&go_param_name, &akind);
            pre_call.extend(pre);
            call_args.extend(a);
            post_call.extend(post);
        }

        let go_ret_ty = self.go_type(&ret_kind);
        let is_void = matches!(ret_kind, FfiKind::Void);

        let (out_decl, call_out, result_expr) = self.c_to_go_output(&ret_kind);
        if !is_void {
            call_args.push(call_out);
        }

        let pre_str = pre_call.join("\n\t");
        let post_str = post_call.join("\n\t");

        // Collect nil checks for binding-type args and KeepAlive calls
        let err_return_prefix = if matches!(ret_kind, FfiKind::Void) {
            "return".to_string()
        } else {
            format!("return {},", self.go_zero(&ret_kind))
        };

        let mut nil_check_lines = Vec::new();
        let mut keep_alive_lines = Vec::new();

        for (aname, atype) in args {
            let akind = self.classify(atype);
            let go_name = aname.to_case(Case::Camel);
            match &akind {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    nil_check_lines.push(format!(
                        "if {go_name} == nil {{\n\t\t{err_return_prefix} fmt.Errorf(\"{go_name} must not be nil\")\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::List(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    nil_check_lines.push(format!(
                        "for i, item := range {go_name} {{\n\t\tif item == nil {{\n\t\t\t{err_return_prefix} fmt.Errorf(\"nil item in {go_name} at index %d\", i)\n\t\t}}\n\t}}"
                    ));
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                FfiKind::Opt(inner) if matches!(inner.as_ref(), FfiKind::Class(_) | FfiKind::Enum(_)) => {
                    keep_alive_lines.push(format!("runtime.KeepAlive({go_name})"));
                }
                _ => {}
            }
        }

        let nil_checks_str = if nil_check_lines.is_empty() {
            String::new()
        } else {
            nil_check_lines.join("\n\t") + "\n\t"
        };

        let keep_alive_str = if keep_alive_lines.is_empty() {
            String::new()
        } else {
            "\n\t".to_string() + &keep_alive_lines.join("\n\t")
        };

        let func_doc = match doc {
            Some(d) => format!("// {go_name} {d}"),
            None => {
                let doc = Self::generate_function_doc(&go_name, args, ret);
                format!("// {go_name} {doc}")
            }
        };

        if is_void {
            self.body.push_str(&format!(
                r#"{func_doc}
func {go_name}({go_params}) error {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{nil_checks_str}{pre_str}
	{post_str}
	ret := C.go_{snake}({call_args}){keep_alive_str}
	if ret != 0 {{
		return lastError()
	}}
	return nil
}}

"#,
                go_params = go_params.join(", "),
                call_args = call_args.join(", "),
            ));
        } else {
            self.body.push_str(&format!(
                r#"{func_doc}
func {go_name}({go_params}) ({go_ret_ty}, error) {{
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	{nil_checks_str}{pre_str}
	{post_str}
	{out_decl}
	ret := C.go_{snake}({call_args}){keep_alive_str}
	if ret != 0 {{
		return {zero}, lastError()
	}}
	{result_expr}
}}

"#,
                go_params = go_params.join(", "),
                call_args = call_args.join(", "),
                zero = self.go_zero(&ret_kind),
            ));
        }
    }

    // ── C extern helpers ───────────────────────────────────────────────

    fn c_extern_param(&self, name: &str, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => format!("int {name}"),
            FfiKind::Prim(t) => {
                let ct = match t.as_str() {
                    "u8" => "uint8_t",
                    "i8" => "int8_t",
                    "u16" => "uint16_t",
                    "i16" => "int16_t",
                    "u32" => "uint32_t",
                    "i32" => "int32_t",
                    "u64" => "uint64_t",
                    "i64" => "int64_t",
                    "usize" => "size_t",
                    "f32" => "float",
                    "f64" => "double",
                    _ => "uint64_t",
                };
                format!("{ct} {name}")
            }
            FfiKind::Bytes | FfiKind::BigInt => {
                format!("const uint8_t* {name}_ptr, size_t {name}_len")
            }
            FfiKind::Str => format!("const char* {name}"),
            FfiKind::Class(_) | FfiKind::Enum(_) => format!("const void* {name}"),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => format!("const void* {name}"),
                FfiKind::Bytes | FfiKind::BigInt => {
                    format!("const uint8_t* {name}_ptr, size_t {name}_len")
                }
                FfiKind::Str => format!("const char* {name}"),
                FfiKind::Bool => format!("int {name}, int {name}_is_some"),
                FfiKind::Prim(t) => {
                    let ct = match t.as_str() {
                        "u8" => "uint8_t",
                        "u16" => "uint16_t",
                        "u32" => "uint32_t",
                        "u64" => "uint64_t",
                        "i32" => "int32_t",
                        "i64" => "int64_t",
                        _ => "uint64_t",
                    };
                    format!("{ct} {name}, int {name}_is_some")
                }
                // List and other complex types: opaque pointer (null = None)
                _ => format!("const void* {name}"),
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    format!("const void** {name}_ptrs, size_t {name}_len")
                }
                _ => format!("const void* {name}"),
            },
        }
    }

    fn c_extern_out(&self, kind: &FfiKind) -> Vec<String> {
        match kind {
            FfiKind::Void => vec![],
            FfiKind::Bool => vec!["int* out".to_string()],
            FfiKind::Prim(t) => {
                let ct = match t.as_str() {
                    "u8" => "uint8_t",
                    "u16" => "uint16_t",
                    "u32" => "uint32_t",
                    "u64" => "uint64_t",
                    "i32" => "int32_t",
                    "i64" => "int64_t",
                    "usize" => "size_t",
                    "f32" => "float",
                    "f64" => "double",
                    _ => "uint64_t",
                };
                vec![format!("{ct}* out")]
            }
            FfiKind::Bytes | FfiKind::BigInt => {
                vec!["uint8_t** out_ptr".to_string(), "size_t* out_len".to_string()]
            }
            FfiKind::Str => vec!["char** out".to_string()],
            FfiKind::Class(_) | FfiKind::Enum(_) => vec!["void** out".to_string()],
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => vec!["void** out".to_string()],
                FfiKind::Bytes | FfiKind::BigInt => {
                    vec!["uint8_t** out_ptr".to_string(), "size_t* out_len".to_string()]
                }
                FfiKind::Str => vec!["char** out".to_string()],
                FfiKind::Bool => vec!["int* out".to_string(), "int* out_is_some".to_string()],
                FfiKind::Prim(t) => {
                    let ct = match t.as_str() {
                        "u8" => "uint8_t",
                        "u32" => "uint32_t",
                        "u64" => "uint64_t",
                        _ => "uint64_t",
                    };
                    vec![format!("{ct}* out"), "int* out_is_some".to_string()]
                }
                // List and other complex types: opaque pointer
                _ => vec!["void** out".to_string()],
            },
            FfiKind::List(_) => vec!["void** out".to_string()],
        }
    }

    // Convert Go param to C call arguments
    fn go_to_c_param(&self, name: &str, kind: &FfiKind) -> (Vec<String>, Vec<String>, Vec<String>) {
        // Returns (pre_call_statements, call_args, post_call_statements)
        match kind {
            FfiKind::Void => (vec![], vec![], vec![]),
            FfiKind::Bool => (
                vec![],
                vec![format!("boolToInt({name})")],
                vec![],
            ),
            FfiKind::Prim(t) => {
                let ct = match t.as_str() {
                    "u8" => "C.uint8_t",
                    "i8" => "C.int8_t",
                    "u16" => "C.uint16_t",
                    "i16" => "C.int16_t",
                    "u32" => "C.uint32_t",
                    "i32" => "C.int32_t",
                    "u64" => "C.uint64_t",
                    "i64" => "C.int64_t",
                    "usize" => "C.size_t",
                    "f32" => "C.float",
                    "f64" => "C.double",
                    _ => "C.uint64_t",
                };
                (vec![], vec![format!("{ct}({name})")], vec![])
            }
            FfiKind::Bytes => {
                let ptr_var = format!("{name}Ptr");
                let len_var = format!("{name}Len");
                (
                    vec![format!(
                        "{ptr_var}, {len_var} := bytesToPtr({name})"
                    )],
                    vec![ptr_var, len_var],
                    vec![],
                )
            }
            FfiKind::Str => {
                let c_var = format!("c{}", name.to_case(Case::Pascal));
                (
                    vec![format!("{c_var} := C.CString({name})")],
                    vec![c_var.clone()],
                    vec![format!("defer C.free(unsafe.Pointer({c_var}))")],
                )
            }
            FfiKind::BigInt => {
                let bytes_var = format!("{name}Bytes");
                let ptr_var = format!("{name}Ptr");
                let len_var = format!("{name}Len");
                (
                    vec![
                        format!("{bytes_var} := bigIntToSignedBytes({name})"),
                        format!("{ptr_var}, {len_var} := bytesToPtr({bytes_var})"),
                    ],
                    vec![ptr_var, len_var],
                    vec![],
                )
            }
            FfiKind::Class(_) | FfiKind::Enum(_) => (
                vec![],
                vec![format!("{name}.ptr")],
                vec![],
            ),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    let ptr_var = format!("{name}Ptr");
                    (
                        vec![format!(
                            "var {ptr_var} unsafe.Pointer\n\tif {name} != nil {{\n\t\t{ptr_var} = {name}.ptr\n\t}}"
                        )],
                        vec![ptr_var],
                        vec![],
                    )
                }
                FfiKind::Bytes => {
                    let ptr_var = format!("{name}Ptr");
                    let len_var = format!("{name}Len");
                    (
                        vec![format!(
                            "{ptr_var}, {len_var} := bytesToPtr({name})"
                        )],
                        vec![ptr_var, len_var],
                        vec![],
                    )
                }
                FfiKind::Str => {
                    let c_var = format!("c{}", name.to_case(Case::Pascal));
                    (
                        vec![format!(
                            "var {c_var} *C.char\n\tif {name} != nil {{\n\t\t{c_var} = C.CString(*{name})\n\t\tdefer C.free(unsafe.Pointer({c_var}))\n\t}}"
                        )],
                        vec![c_var],
                        vec![],
                    )
                }
                FfiKind::Bool => {
                    let val_var = format!("{name}Val");
                    let some_var = format!("{name}IsSome");
                    (
                        vec![format!(
                            "var {val_var} C.int\n\t\
                             var {some_var} C.int\n\t\
                             if {name} != nil {{\n\t\t\
                                 {val_var} = boolToInt(*{name})\n\t\t\
                                 {some_var} = 1\n\t\
                             }}"
                        )],
                        vec![val_var, some_var],
                        vec![],
                    )
                }
                FfiKind::Prim(t) => {
                    let ct = match t.as_str() {
                        "u8" => "C.uint8_t",
                        "u32" => "C.uint32_t",
                        "u64" => "C.uint64_t",
                        _ => "C.uint64_t",
                    };
                    let val_var = format!("{name}Val");
                    let some_var = format!("{name}IsSome");
                    (
                        vec![format!(
                            "var {val_var} {ct}\n\t\
                             var {some_var} C.int\n\t\
                             if {name} != nil {{\n\t\t\
                                 {val_var} = {ct}(*{name})\n\t\t\
                                 {some_var} = 1\n\t\
                             }}"
                        )],
                        vec![val_var, some_var],
                        vec![],
                    )
                }
                FfiKind::BigInt => {
                    let ptr_var = format!("{name}Ptr");
                    let len_var = format!("{name}Len");
                    (
                        vec![format!(
                            "var {ptr_var} *C.uint8_t\n\t\
                             var {len_var} C.size_t\n\t\
                             if {name} != nil {{\n\t\t\
                                 {name}Bytes := bigIntToSignedBytes({name})\n\t\t\
                                 {ptr_var}, {len_var} = bytesToPtr({name}Bytes)\n\t\
                             }}"
                        )],
                        vec![ptr_var, len_var],
                        vec![],
                    )
                }
                // List and other complex types: pass opaque pointer directly
                _ => (
                    vec![],
                    vec![format!("{name}")],
                    vec![],
                ),
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(_) | FfiKind::Enum(_) => {
                    let ptrs_var = format!("{name}Ptrs");
                    let len_var = format!("{name}Len");
                    (
                        vec![format!(
                            "{ptrs_var} := make([]unsafe.Pointer, len({name}))\n\t\
                             for i, item := range {name} {{\n\t\t\
                                 {ptrs_var}[i] = item.ptr\n\t\
                             }}\n\t\
                             var {ptrs_var}C *unsafe.Pointer\n\t\
                             if len({ptrs_var}) > 0 {{\n\t\t\
                                 {ptrs_var}C = &{ptrs_var}[0]\n\t\
                             }}\n\t\
                             {len_var} := C.size_t(len({name}))"
                        )],
                        vec![
                            format!("(*unsafe.Pointer)(unsafe.Pointer({ptrs_var}C))"),
                            len_var,
                        ],
                        vec![],
                    )
                }
                _ => (
                    vec![],
                    vec![format!("{name}")],
                    vec![],
                ),
            },
        }
    }

    // Convert C output to Go result
    fn c_to_go_output(&self, kind: &FfiKind) -> (String, String, String) {
        // Returns (declaration, call_out_arg, result_expression)
        match kind {
            FfiKind::Void => (String::new(), String::new(), String::new()),
            FfiKind::Bool => (
                "var cOut C.int".to_string(),
                "&cOut".to_string(),
                "return cOut != 0, nil".to_string(),
            ),
            FfiKind::Prim(t) => {
                let ct = match t.as_str() {
                    "u8" => "C.uint8_t",
                    "i8" => "C.int8_t",
                    "u16" => "C.uint16_t",
                    "i16" => "C.int16_t",
                    "u32" => "C.uint32_t",
                    "i32" => "C.int32_t",
                    "u64" => "C.uint64_t",
                    "i64" => "C.int64_t",
                    "usize" => "C.size_t",
                    "f32" => "C.float",
                    "f64" => "C.double",
                    _ => "C.uint64_t",
                };
                let gt = self.go_type(kind);
                (
                    format!("var cOut {ct}"),
                    "&cOut".to_string(),
                    format!("return {gt}(cOut), nil"),
                )
            }
            FfiKind::Bytes => (
                "var outPtr *C.uint8_t\n\tvar outLen C.size_t".to_string(),
                "&outPtr, &outLen".to_string(),
                "result := C.GoBytes(unsafe.Pointer(outPtr), C.int(outLen))\n\t\
                 C.go_free_bytes((*C.uint8_t)(unsafe.Pointer(outPtr)), outLen)\n\t\
                 return result, nil"
                    .to_string(),
            ),
            FfiKind::Str => (
                "var cOut *C.char".to_string(),
                "&cOut".to_string(),
                "result := C.GoString(cOut)\n\tC.go_free_string(cOut)\n\treturn result, nil"
                    .to_string(),
            ),
            FfiKind::BigInt => (
                "var outPtr *C.uint8_t\n\tvar outLen C.size_t".to_string(),
                "&outPtr, &outLen".to_string(),
                "bytes := C.GoBytes(unsafe.Pointer(outPtr), C.int(outLen))\n\t\
                 C.go_free_bytes((*C.uint8_t)(unsafe.Pointer(outPtr)), outLen)\n\t\
                 result := bigIntFromSignedBytes(bytes)\n\t\
                 return result, nil"
                    .to_string(),
            ),
            FfiKind::Class(cls) | FfiKind::Enum(cls) => (
                "var out unsafe.Pointer".to_string(),
                "&out".to_string(),
                format!(
                    "obj := &{cls}{{ptr: out}}\n\t\
                     runtime.SetFinalizer(obj, (*{cls}).Free)\n\t\
                     return obj, nil"
                ),
            ),
            FfiKind::Opt(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => (
                    "var out unsafe.Pointer".to_string(),
                    "&out".to_string(),
                    format!(
                        "if out == nil {{\n\t\treturn nil, nil\n\t}}\n\t\
                         obj := &{cls}{{ptr: out}}\n\t\
                         runtime.SetFinalizer(obj, (*{cls}).Free)\n\t\
                         return obj, nil"
                    ),
                ),
                FfiKind::Bytes => (
                    "var outPtr *C.uint8_t\n\tvar outLen C.size_t".to_string(),
                    "&outPtr, &outLen".to_string(),
                    "if outPtr == nil {\n\t\treturn nil, nil\n\t}\n\t\
                     result := C.GoBytes(unsafe.Pointer(outPtr), C.int(outLen))\n\t\
                     C.go_free_bytes((*C.uint8_t)(unsafe.Pointer(outPtr)), outLen)\n\t\
                     return result, nil"
                        .to_string(),
                ),
                FfiKind::Str => (
                    "var cOut *C.char".to_string(),
                    "&cOut".to_string(),
                    "if cOut == nil {\n\t\treturn nil, nil\n\t}\n\t\
                     s := C.GoString(cOut)\n\tC.go_free_string(cOut)\n\treturn &s, nil"
                        .to_string(),
                ),
                FfiKind::Bool => (
                    "var cOut C.int\n\tvar cOutIsSome C.int".to_string(),
                    "&cOut, &cOutIsSome".to_string(),
                    "if cOutIsSome == 0 {\n\t\treturn nil, nil\n\t}\n\t\
                     v := cOut != 0\n\treturn &v, nil"
                        .to_string(),
                ),
                FfiKind::Prim(t) => {
                    let ct = match t.as_str() {
                        "u8" => "C.uint8_t",
                        "u32" => "C.uint32_t",
                        "u64" => "C.uint64_t",
                        _ => "C.uint64_t",
                    };
                    let gt = self.go_type(&FfiKind::Prim(t.clone()));
                    (
                        format!("var cOut {ct}\n\tvar cOutIsSome C.int"),
                        "&cOut, &cOutIsSome".to_string(),
                        format!(
                            "if cOutIsSome == 0 {{\n\t\treturn nil, nil\n\t}}\n\t\
                             v := {gt}(cOut)\n\treturn &v, nil"
                        ),
                    )
                }
                FfiKind::BigInt => (
                    "var outPtr *C.uint8_t\n\tvar outLen C.size_t".to_string(),
                    "&outPtr, &outLen".to_string(),
                    "if outPtr == nil {\n\t\treturn nil, nil\n\t}\n\t\
                     bytes := C.GoBytes(unsafe.Pointer(outPtr), C.int(outLen))\n\t\
                     C.go_free_bytes((*C.uint8_t)(unsafe.Pointer(outPtr)), outLen)\n\t\
                     result := bigIntFromSignedBytes(bytes)\n\t\
                     return result, nil"
                        .to_string(),
                ),
                // List and other complex types: opaque pointer (null = None)
                _ => (
                    "var out unsafe.Pointer".to_string(),
                    "&out".to_string(),
                    "if out == nil {\n\t\treturn nil, nil\n\t}\n\t\
                     return out, nil"
                        .to_string(),
                ),
            },
            FfiKind::List(inner) => match inner.as_ref() {
                FfiKind::Class(cls) | FfiKind::Enum(cls) => (
                    "var out unsafe.Pointer".to_string(),
                    "&out".to_string(),
                    format!(
                        "listLen := C.go_{snake}_list_len(out)\n\t\
                         result := make([]*{cls}, listLen)\n\t\
                         for i := range result {{\n\t\t\
                             var itemPtr unsafe.Pointer\n\t\t\
                             if ret := C.go_{snake}_list_get(out, C.size_t(i), &itemPtr); ret != 0 {{\n\t\t\t\
                                 C.go_{snake}_list_free(out)\n\t\t\t\
                                 return nil, lastError()\n\t\t\
                             }}\n\t\t\
                             result[i] = &{cls}{{ptr: itemPtr}}\n\t\t\
                             runtime.SetFinalizer(result[i], (*{cls}).Free)\n\t\
                         }}\n\t\
                         C.go_{snake}_list_free(out)\n\t\
                         return result, nil",
                        snake = cls.to_case(Case::Snake),
                    ),
                ),
                _ => (
                    "var out unsafe.Pointer".to_string(),
                    "&out".to_string(),
                    "return out, nil".to_string(),
                ),
            },
        }
    }

    fn go_zero(&self, kind: &FfiKind) -> String {
        match kind {
            FfiKind::Void => String::new(),
            FfiKind::Bool => "false".to_string(),
            FfiKind::Prim(_) => "0".to_string(),
            FfiKind::Bytes => "nil".to_string(),
            FfiKind::Str => "\"\"".to_string(),
            FfiKind::BigInt => "nil".to_string(),
            FfiKind::Class(_) | FfiKind::Enum(_) => "nil".to_string(),
            FfiKind::Opt(_) => "nil".to_string(),
            FfiKind::List(_) => "nil".to_string(),
        }
    }
}

// ── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let root = env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());
    let root = Path::new(&root);

    let (bindy, bindings) = load_bindings(root);
    let mappings = build_mappings(&bindy);

    // Collect class and enum names
    let classes: HashSet<String> = bindings
        .iter()
        .filter_map(|(name, b)| {
            if matches!(b, Binding::Class { .. }) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    let enums: HashSet<String> = bindings
        .iter()
        .filter_map(|(name, b)| {
            if matches!(b, Binding::Enum { .. }) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    // Validate all identifiers from binding JSON
    for (name, binding) in &bindings {
        validate_identifier(name);
        match binding {
            Binding::Class { fields, methods, .. } => {
                for fname in fields.keys() {
                    validate_identifier(fname);
                }
                for (mname, method) in methods {
                    validate_identifier(mname);
                    for aname in method.args.keys() {
                        validate_identifier(aname);
                    }
                }
            }
            Binding::Enum { values, .. } => {
                for v in values {
                    validate_identifier(v);
                }
            }
            Binding::Function { args, .. } => {
                for aname in args.keys() {
                    validate_identifier(aname);
                }
            }
        }
    }

    // ── Generate Rust FFI ──────────────────────────────────────────────
    let mut rust = RustGen::new(&bindy.entrypoint, mappings.clone(), classes.clone(), enums.clone());
    rust.write_prelude();

    for (name, binding) in &bindings {
        match binding {
            Binding::Class {
                new,
                fields,
                methods,
                remote,
                ..
            } => {
                rust.write_class(name, *new, fields, methods, *remote);
            }
            Binding::Enum { values, .. } => {
                rust.write_enum(name, values);
            }
            Binding::Function { args, ret, .. } => {
                rust.write_function(name, args, ret);
            }
        }
    }

    let rust_path = root.join("go/src/generated.rs");
    fs::write(&rust_path, &rust.out)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", rust_path.display()));
    eprintln!("Wrote {}", rust_path.display());

    // ── Generate Go ────────────────────────────────────────────────────
    let mut go = GoGen::new(mappings, classes, enums);
    go.write_prelude();

    for (name, binding) in &bindings {
        match binding {
            Binding::Class {
                doc,
                new,
                fields,
                methods,
                remote,
                ..
            } => {
                go.write_class(name, doc, *new, fields, methods, *remote);
            }
            Binding::Enum { doc, values } => {
                go.write_enum(name, doc, values);
            }
            Binding::Function { doc, args, ret } => {
                go.write_function(name, doc, args, ret);
            }
        }
    }

    // Assemble Go file
    let needs_big = go.body.contains("big.Int");
    let mut go_file = String::new();
    go_file.push_str("// AUTO-GENERATED by go-codegen. DO NOT EDIT.\n");
    go_file.push_str("package chiawalletsdk\n\n");
    go_file.push_str("/*\n");
    go_file.push_str("#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/libs/linux_amd64 -lchia_wallet_sdk_go -lm -ldl -lpthread\n");
    go_file.push_str("#cgo linux,arm64 LDFLAGS: -L${SRCDIR}/libs/linux_arm64 -lchia_wallet_sdk_go -lm -ldl -lpthread\n");
    go_file.push_str("#cgo android,arm64 LDFLAGS: -L${SRCDIR}/libs/android_arm64 -lchia_wallet_sdk_go -lm -ldl\n");
    go_file.push_str("#cgo darwin,amd64 LDFLAGS: -L${SRCDIR}/libs/darwin_amd64 -lchia_wallet_sdk_go -framework Security -framework CoreFoundation\n");
    go_file.push_str("#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/libs/darwin_arm64 -lchia_wallet_sdk_go -framework Security -framework CoreFoundation\n");
    go_file.push_str("#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/libs/windows_amd64 -lchia_wallet_sdk_go -lws2_32 -lbcrypt -luserenv -lntdll\n");
    go_file.push_str("#cgo windows,arm64 LDFLAGS: -L${SRCDIR}/libs/windows_arm64 -lchia_wallet_sdk_go -lws2_32 -lbcrypt -luserenv -lntdll\n");
    go_file.push_str("#include <stdlib.h>\n");
    go_file.push_str("#include <stdint.h>\n\n");
    go_file.push_str(&go.externs);

    // Emit noescape/nocallback directives only for C functions actually called from Go code.
    // The Rust FFI functions never call back into Go and copy all Go data immediately.
    for fname in &go.c_func_names {
        if go.body.contains(&format!("C.{fname}(")) {
            go_file.push_str(&format!("#cgo noescape {fname}\n"));
            go_file.push_str(&format!("#cgo nocallback {fname}\n"));
        }
    }

    go_file.push_str("*/\n");
    go_file.push_str("import \"C\"\n");
    go_file.push_str("import (\n");
    go_file.push_str("\t\"fmt\"\n");
    if needs_big {
        go_file.push_str("\t\"math/big\"\n");
    }
    go_file.push_str("\t\"runtime\"\n");
    go_file.push_str("\t\"sync\"\n");
    go_file.push_str("\t\"unsafe\"\n");
    go_file.push_str(")\n\n");
    go_file.push_str("func boolToInt(b bool) C.int {\n");
    go_file.push_str("\tif b {\n\t\treturn 1\n\t}\n\treturn 0\n}\n");
    go_file.push_str(&go.body);

    // Clean up blank lines with only whitespace and collapse consecutive blank lines (keeps gofmt happy)
    let mut cleaned = Vec::new();
    let mut prev_blank = false;
    for line in go_file.lines() {
        let is_blank = line.chars().all(|c| c.is_whitespace());
        if is_blank {
            if !prev_blank {
                cleaned.push("");
            }
            prev_blank = true;
        } else {
            cleaned.push(line);
            prev_blank = false;
        }
    }
    // Remove trailing empty lines before joining
    while cleaned.last() == Some(&"") {
        cleaned.pop();
    }
    let go_file = cleaned.join("\n") + "\n";

    let go_path = root.join("go/chiawalletsdk/generated.go");
    fs::write(&go_path, &go_file)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", go_path.display()));
    eprintln!("Wrote {}", go_path.display());
}
