use std::{fs, mem, path::Path};

use chia_sdk_bindings::CONSTANTS;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use indoc::formatdoc;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde::{Deserialize, Serialize};
use syn::{parse_str, Ident, LitStr, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bindy {
    entrypoint: String,
    pymodule: String,
    #[serde(default)]
    type_groups: IndexMap<String, Vec<String>>,
    #[serde(default)]
    shared: IndexMap<String, String>,
    #[serde(default)]
    napi: IndexMap<String, String>,
    #[serde(default)]
    wasm: IndexMap<String, String>,
    #[serde(default)]
    wasm_stubs: IndexMap<String, String>,
    #[serde(default)]
    pyo3: IndexMap<String, String>,
    #[serde(default)]
    pyo3_stubs: IndexMap<String, String>,
    #[serde(default)]
    clvm_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Binding {
    Class {
        #[serde(default)]
        new: bool,
        #[serde(default)]
        fields: IndexMap<String, String>,
        #[serde(default)]
        methods: IndexMap<String, Method>,
        #[serde(default)]
        remote: bool,
        #[serde(default)]
        no_wasm: bool,
    },
    Enum {
        values: Vec<String>,
    },
    Function {
        #[serde(default)]
        args: IndexMap<String, String>,
        #[serde(rename = "return")]
        ret: Option<String>,
    },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Method {
    #[serde(rename = "type")]
    kind: MethodKind,
    args: IndexMap<String, String>,
    #[serde(rename = "return")]
    ret: Option<String>,
    #[serde(default)]
    stub_only: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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

fn load_bindings(path: &str) -> (Bindy, IndexMap<String, Binding>) {
    let source = fs::read_to_string(path).unwrap();

    let bindy: Bindy = serde_json::from_str(&source).unwrap();

    let mut bindings = IndexMap::new();

    let mut dir: Vec<_> = fs::read_dir(Path::new(path).parent().unwrap().join("bindings"))
        .unwrap()
        .map(|p| p.unwrap())
        .collect();

    dir.sort_by_key(|p| p.path().file_name().unwrap().to_str().unwrap().to_string());

    for path in dir {
        if path.path().extension().unwrap() == "json" {
            let source = fs::read_to_string(path.path()).unwrap();
            let contents: IndexMap<String, Binding> = serde_json::from_str(&source).unwrap();
            bindings.extend(contents);
        }
    }

    if let Binding::Class { methods, .. } =
        &mut bindings.get_mut("Constants").expect("Constants not found")
    {
        for &name in CONSTANTS {
            methods.insert(
                name.to_string(),
                Method {
                    kind: MethodKind::Static,
                    args: IndexMap::new(),
                    ret: Some("SerializedProgram".to_string()),
                    stub_only: false,
                },
            );

            methods.insert(
                format!("{name}_hash"),
                Method {
                    kind: MethodKind::Static,
                    args: IndexMap::new(),
                    ret: Some("TreeHash".to_string()),
                    stub_only: false,
                },
            );
        }
    }

    if let Binding::Class { methods, .. } = &mut bindings.get_mut("Clvm").expect("Clvm not found") {
        for &name in CONSTANTS {
            methods.insert(
                name.to_string(),
                Method {
                    kind: MethodKind::Normal,
                    args: IndexMap::new(),
                    ret: Some("Program".to_string()),
                    stub_only: false,
                },
            );
        }
    }

    (bindy, bindings)
}

fn build_base_mappings(
    bindy: &Bindy,
    mappings: &mut IndexMap<String, String>,
    stubs: &mut IndexMap<String, String>,
) {
    for (name, value) in &bindy.shared {
        if !mappings.contains_key(name) {
            mappings.insert(name.clone(), value.clone());
        }

        if !stubs.contains_key(name) {
            stubs.insert(name.clone(), value.clone());
        }
    }

    for (name, group) in &bindy.type_groups {
        if let Some(value) = stubs.shift_remove(name) {
            for ty in group {
                if !stubs.contains_key(ty) {
                    stubs.insert(ty.clone(), value.clone());
                }
            }
        }

        if let Some(value) = mappings.shift_remove(name) {
            for ty in group {
                if !mappings.contains_key(ty) {
                    mappings.insert(ty.clone(), value.clone());
                }

                if !stubs.contains_key(ty) {
                    stubs.insert(ty.clone(), value.clone());
                }
            }
        }
    }
}

#[proc_macro]
pub fn bindy_napi(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let (bindy, bindings) = load_bindings(&input);

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut base_mappings = bindy.napi.clone();
    build_base_mappings(&bindy, &mut base_mappings, &mut IndexMap::new());

    let mut non_async_param_mappings = base_mappings.clone();
    let mut async_param_mappings = base_mappings.clone();
    let mut return_mappings = base_mappings;

    for (name, binding) in &bindings {
        if matches!(binding, Binding::Class { .. }) {
            non_async_param_mappings.insert(
                name.clone(),
                format!("napi::bindgen_prelude::ClassInstance<'_, {name}>"),
            );
            async_param_mappings.insert(name.clone(), format!("&'_ {name}"));
        }
    }

    // We accept Uint8Array as parameters for flexibility, but return Buffer for ease of use
    // For context, Buffer is a subclass of Uint8Array with more methods, and is commonly used in Node.js
    for ty in return_mappings.values_mut() {
        if ty.as_str() == "napi::bindgen_prelude::Uint8Array" {
            *ty = "napi::bindgen_prelude::Buffer".to_string();
        }
    }

    let mut output = quote!();

    for (name, binding) in bindings {
        match binding {
            Binding::Class {
                new,
                remote,
                methods,
                fields,
                no_wasm: _,
            } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_struct_ident = quote!( #entrypoint::#bound_ident );
                let fully_qualified_ident = if remote {
                    let ext_ident = Ident::new(&format!("{name}Ext"), Span::mixed_site());
                    quote!( <#rust_struct_ident as #entrypoint::#ext_ident> )
                } else {
                    quote!( #rust_struct_ident )
                };

                let mut method_tokens = quote! {
                    #[napi]
                    pub fn clone(&self) -> Self {
                        Clone::clone(self)
                    }
                };

                for (name, method) in methods {
                    if method.stub_only {
                        continue;
                    }

                    let method_ident = Ident::new(&name, Span::mixed_site());

                    let param_mappings = if matches!(method.kind, MethodKind::Async)
                        || matches!(method.kind, MethodKind::AsyncFactory)
                    {
                        &async_param_mappings
                    } else {
                        &non_async_param_mappings
                    };

                    let arg_idents = method
                        .args
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = method
                        .args
                        .values()
                        .map(|v| {
                            parse_str::<Type>(apply_mappings(v, param_mappings).as_str()).unwrap()
                        })
                        .collect::<Vec<_>>();

                    let ret = parse_str::<Type>(
                        apply_mappings(
                            method.ret.as_deref().unwrap_or(
                                if matches!(
                                    method.kind,
                                    MethodKind::Constructor
                                        | MethodKind::Factory
                                        | MethodKind::AsyncFactory
                                ) {
                                    "Self"
                                } else {
                                    "()"
                                },
                            ),
                            &return_mappings,
                        )
                        .as_str(),
                    )
                    .unwrap();

                    let napi_attr = match method.kind {
                        MethodKind::Constructor => quote!(#[napi(constructor)]),
                        MethodKind::Static => quote!(#[napi]),
                        MethodKind::Factory | MethodKind::AsyncFactory => quote!(#[napi(factory)]),
                        MethodKind::Normal | MethodKind::Async | MethodKind::ToString => {
                            quote!(#[napi])
                        }
                    };

                    match method.kind {
                        MethodKind::Constructor | MethodKind::Static | MethodKind::Factory => {
                            method_tokens.extend(quote! {
                                #napi_attr
                                pub fn #method_ident(
                                    env: Env,
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    )?, &bindy::NapiReturnContext(env))?)
                                }
                            });
                        }
                        MethodKind::AsyncFactory => {
                            method_tokens.extend(quote! {
                                #napi_attr
                                pub async fn #method_ident(
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    ).await?, &bindy::NapiAsyncReturnContext)?)
                                }
                            });
                        }
                        MethodKind::Normal | MethodKind::ToString => {
                            method_tokens.extend(quote! {
                                #napi_attr
                                pub fn #method_ident(
                                    &self,
                                    env: Env,
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(#fully_qualified_ident::#method_ident(
                                        &self.0,
                                        #( bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    )?, &bindy::NapiReturnContext(env))?)
                                }
                            });
                        }
                        MethodKind::Async => {
                            method_tokens.extend(quote! {
                                #napi_attr
                                pub async fn #method_ident(
                                    &self,
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(self.0.#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    ).await?, &bindy::NapiAsyncReturnContext)?)
                                }
                            });
                        }
                    }
                }

                let mut field_tokens = quote!();

                for (name, ty) in &fields {
                    let ident = Ident::new(name, Span::mixed_site());
                    let get_ident = Ident::new(&format!("get_{name}"), Span::mixed_site());
                    let set_ident = Ident::new(&format!("set_{name}"), Span::mixed_site());
                    let get_ty =
                        parse_str::<Type>(apply_mappings(ty, &return_mappings).as_str()).unwrap();
                    let set_ty =
                        parse_str::<Type>(apply_mappings(ty, &non_async_param_mappings).as_str())
                            .unwrap();

                    field_tokens.extend(quote! {
                        #[napi(getter)]
                        pub fn #get_ident(&self, env: Env) -> napi::Result<#get_ty> {
                            Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(self.0.#ident.clone(), &bindy::NapiReturnContext(env))?)
                        }

                        #[napi(setter)]
                        pub fn #set_ident(&mut self, env: Env, value: #set_ty) -> napi::Result<()> {
                            self.0.#ident = bindy::IntoRust::<_, _, bindy::Napi>::into_rust(value, &bindy::NapiParamContext)?;
                            Ok(())
                        }
                    });
                }

                if new {
                    let arg_idents = fields
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = fields
                        .values()
                        .map(|v| {
                            parse_str::<Type>(apply_mappings(v, &non_async_param_mappings).as_str())
                                .unwrap()
                        })
                        .collect::<Vec<_>>();

                    method_tokens.extend(quote! {
                        #[napi(constructor)]
                        pub fn new(
                            env: Env,
                            #( #arg_idents: #arg_types ),*
                        ) -> napi::Result<Self> {
                            Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(#rust_struct_ident {
                                #(#arg_idents: bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)?),*
                            }, &bindy::NapiReturnContext(env))?)
                        }
                    });
                }

                output.extend(quote! {
                    #[napi_derive::napi]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_struct_ident);

                    #[napi_derive::napi]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_struct_ident, T, bindy::Napi> for #bound_ident {
                        fn from_rust(value: #rust_struct_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_struct_ident, T, bindy::Napi> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_struct_ident> {
                            Ok(self.0)
                        }
                    }
                });
            }
            Binding::Enum { values } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let value_idents = values
                    .iter()
                    .map(|v| Ident::new(v, Span::mixed_site()))
                    .collect::<Vec<_>>();

                output.extend(quote! {
                    #[napi_derive::napi]
                    pub enum #bound_ident {
                        #( #value_idents ),*
                    }

                    impl<T> bindy::FromRust<#rust_ident, T, bindy::Napi> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(match value {
                                #( #rust_ident::#value_idents => Self::#value_idents ),*
                            })
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T, bindy::Napi> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
                            Ok(match self {
                                #( Self::#value_idents => #rust_ident::#value_idents ),*
                            })
                        }
                    }
                });
            }
            Binding::Function { args, ret } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let ident = Ident::new(&name, Span::mixed_site());

                let arg_idents = args
                    .keys()
                    .map(|k| Ident::new(k, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let arg_types = args
                    .values()
                    .map(|v| {
                        parse_str::<Type>(apply_mappings(v, &non_async_param_mappings).as_str())
                            .unwrap()
                    })
                    .collect::<Vec<_>>();

                let ret = parse_str::<Type>(
                    apply_mappings(ret.as_deref().unwrap_or("()"), &return_mappings).as_str(),
                )
                .unwrap();

                output.extend(quote! {
                    #[napi_derive::napi]
                    pub fn #bound_ident(
                        env: Env,
                        #( #arg_idents: #arg_types ),*
                    ) -> napi::Result<#ret> {
                        Ok(bindy::FromRust::<_, _, bindy::Napi>::from_rust(#entrypoint::#ident(
                            #( bindy::IntoRust::<_, _, bindy::Napi>::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                        )?, &bindy::NapiReturnContext(env))?)
                    }
                });
            }
        }
    }

    let clvm_types = bindy
        .clvm_types
        .iter()
        .map(|s| Ident::new(s, Span::mixed_site()))
        .collect::<Vec<_>>();

    let mut value_index = 1;
    let mut value_idents = Vec::new();
    let mut remaining_clvm_types = clvm_types.clone();

    while !remaining_clvm_types.is_empty() {
        let value_ident = Ident::new(&format!("Value{value_index}"), Span::mixed_site());
        value_index += 1;

        let consumed = if remaining_clvm_types.len() <= 26 {
            let either_ident = Ident::new(
                &format!("Either{}", remaining_clvm_types.len()),
                Span::mixed_site(),
            );

            output.extend(quote! {
                type #value_ident<'a> = #either_ident< #( ClassInstance<'a, #remaining_clvm_types > ),* >;
            });

            mem::take(&mut remaining_clvm_types)
        } else {
            let either_ident = Ident::new("Either26", Span::mixed_site());
            let next_value_ident = Ident::new(&format!("Value{value_index}"), Span::mixed_site());
            let next_25 = remaining_clvm_types.drain(..25).collect::<Vec<_>>();

            output.extend(quote! {
                type #value_ident<'a> = #either_ident< #( ClassInstance<'a, #next_25 > ),*, #next_value_ident<'a> >;
            });

            next_25
        };

        value_idents.push((value_ident, consumed));
    }

    let mut extractor = proc_macro2::TokenStream::new();

    for (i, (value_ident, consumed)) in value_idents.into_iter().rev().enumerate() {
        let chain = (i > 0).then(|| quote!( #value_ident::Z(value) => #extractor, ));

        let items = consumed
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let letter = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
                    .chars()
                    .nth(i)
                    .unwrap()
                    .to_string();
                let letter = Ident::new(&letter, Span::mixed_site());
                quote!( #value_ident::#letter(value) => ClvmType::#ty((*value).clone()) )
            })
            .collect::<Vec<_>>();

        extractor = quote! {
            match value {
                #( #items, )*
                #chain
            }
        };
    }

    output.extend(quote! {
        enum ClvmType {
            #( #clvm_types ( #clvm_types ), )*
        }

        fn extract_clvm_type(value: Value1) -> ClvmType {
            #extractor
        }
    });

    output.into()
}

#[proc_macro]
pub fn bindy_wasm(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let (bindy, bindings) = load_bindings(&input);

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut base_mappings = bindy.wasm.clone();
    let mut stubs = bindy.wasm_stubs.clone();
    build_base_mappings(&bindy, &mut base_mappings, &mut stubs);

    let mut param_mappings = base_mappings.clone();
    let return_mappings = base_mappings;

    for (name, binding) in &bindings {
        if matches!(binding, Binding::Class { no_wasm: false, .. }) {
            param_mappings.insert(
                format!("Option<Vec<{name}>>"),
                format!("&{name}OptionArrayType"),
            );
            param_mappings.insert(format!("Option<{name}>"), format!("&{name}OptionType"));
            param_mappings.insert(format!("Vec<{name}>"), format!("&{name}ArrayType"));
            param_mappings.insert(name.clone(), format!("&{name}"));

            stubs.insert(
                format!("Option<Vec<{name}>>"),
                format!("{name}[] | undefined"),
            );
            stubs.insert(format!("Option<{name}>"), format!("{name} | undefined"));
            stubs.insert(format!("Vec<{name}>"), format!("{name}[]"));
        }
    }

    let mut output = quote!();
    let mut js_types = quote!();

    let mut classes = String::new();
    let mut functions = String::new();

    for (name, binding) in bindings {
        match binding {
            Binding::Class {
                new,
                remote,
                methods,
                fields,
                no_wasm,
            } => {
                if no_wasm {
                    continue;
                }

                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_struct_ident = quote!( #entrypoint::#bound_ident );
                let fully_qualified_ident = if remote {
                    let ext_ident = Ident::new(&format!("{name}Ext"), Span::mixed_site());
                    quote!( <#rust_struct_ident as #entrypoint::#ext_ident> )
                } else {
                    quote!( #rust_struct_ident )
                };

                let mut method_tokens = quote! {
                    #[wasm_bindgen]
                    pub fn clone(&self) -> Self {
                        Clone::clone(self)
                    }
                };

                let mut method_stubs = String::new();

                let class_name = name.clone();

                for (name, method) in methods {
                    if !method.stub_only {
                        let js_name = name.to_case(Case::Camel);
                        let method_ident = Ident::new(&name, Span::mixed_site());

                        let arg_attrs = method
                            .args
                            .keys()
                            .map(|k| {
                                let js_name = k.to_case(Case::Camel);
                                quote!( #[wasm_bindgen(js_name = #js_name)] )
                            })
                            .collect::<Vec<_>>();

                        let arg_idents = method
                            .args
                            .keys()
                            .map(|k| Ident::new(k, Span::mixed_site()))
                            .collect::<Vec<_>>();

                        let arg_types = method
                            .args
                            .values()
                            .map(|v| {
                                parse_str::<Type>(apply_mappings(v, &param_mappings).as_str())
                                    .unwrap()
                            })
                            .collect::<Vec<_>>();

                        let ret = parse_str::<Type>(
                            apply_mappings(
                                method.ret.as_deref().unwrap_or(
                                    if matches!(
                                        method.kind,
                                        MethodKind::Constructor
                                            | MethodKind::Factory
                                            | MethodKind::AsyncFactory
                                    ) {
                                        "Self"
                                    } else {
                                        "()"
                                    },
                                ),
                                &return_mappings,
                            )
                            .as_str(),
                        )
                        .unwrap();

                        let wasm_attr = if let MethodKind::Constructor = method.kind {
                            quote!(#[wasm_bindgen(constructor)])
                        } else {
                            quote!(#[wasm_bindgen(js_name = #js_name)])
                        };

                        match method.kind {
                            MethodKind::Constructor | MethodKind::Static | MethodKind::Factory => {
                                method_tokens.extend(quote! {
                                #wasm_attr
                                pub fn #method_ident(
                                    #( #arg_attrs #arg_idents: #arg_types ),*
                                ) -> Result<#ret, wasm_bindgen::JsError> {
                                    Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    )?, &bindy::WasmContext)?)
                                }
                            });
                            }
                            MethodKind::AsyncFactory => {
                                method_tokens.extend(quote! {
                                #wasm_attr
                                pub async fn #method_ident(
                                    #( #arg_attrs #arg_idents: #arg_types ),*
                                ) -> Result<#ret, wasm_bindgen::JsError> {
                                    Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    ).await?, &bindy::WasmContext)?)
                                }
                            });
                            }
                            MethodKind::Normal | MethodKind::ToString => {
                                method_tokens.extend(quote! {
                                #wasm_attr
                                pub fn #method_ident(
                                    &self,
                                    #( #arg_attrs #arg_idents: #arg_types ),*
                                ) -> Result<#ret, wasm_bindgen::JsError> {
                                    Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(#fully_qualified_ident::#method_ident(
                                        &self.0,
                                        #( bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    )?, &bindy::WasmContext)?)
                                }
                            });
                            }
                            MethodKind::Async => {
                                method_tokens.extend(quote! {
                                #wasm_attr
                                pub async fn #method_ident(
                                    &self,
                                    #( #arg_attrs #arg_idents: #arg_types ),*
                                ) -> Result<#ret, wasm_bindgen::JsError> {
                                    Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(self.0.#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    ).await?, &bindy::WasmContext)?)
                                }
                            });
                            }
                        }
                    }

                    let js_name = if matches!(method.kind, MethodKind::Constructor) {
                        "constructor".to_string()
                    } else {
                        name.to_case(Case::Camel)
                    };

                    let arg_stubs = function_args(&method.args, &stubs, MappingFlavor::JavaScript);

                    let mut ret_stub = apply_mappings_with_flavor(
                        method.ret.as_deref().unwrap_or("()"),
                        &stubs,
                        MappingFlavor::JavaScript,
                    );

                    match method.kind {
                        MethodKind::Async => ret_stub = format!("Promise<{ret_stub}>"),
                        MethodKind::Factory => {
                            ret_stub.clone_from(&class_name);
                        }
                        MethodKind::AsyncFactory => {
                            ret_stub = format!("Promise<{class_name}>");
                        }
                        _ => {}
                    }

                    let prefix = match method.kind {
                        MethodKind::Factory | MethodKind::Static | MethodKind::AsyncFactory => {
                            "static "
                        }
                        _ => "",
                    };

                    let ret_stub = if matches!(method.kind, MethodKind::Constructor) {
                        String::new()
                    } else {
                        format!(": {ret_stub}")
                    };

                    method_stubs.push_str(&formatdoc! {"
                        {prefix}{js_name}({arg_stubs}){ret_stub};
                    "});
                }

                let mut field_tokens = quote!();
                let mut field_stubs = String::new();

                for (name, ty) in &fields {
                    let js_name = name.to_case(Case::Camel);
                    let ident = Ident::new(name, Span::mixed_site());
                    let get_ident = Ident::new(&format!("get_{name}"), Span::mixed_site());
                    let set_ident = Ident::new(&format!("set_{name}"), Span::mixed_site());
                    let param_type =
                        parse_str::<Type>(apply_mappings(ty, &param_mappings).as_str()).unwrap();
                    let return_type =
                        parse_str::<Type>(apply_mappings(ty, &return_mappings).as_str()).unwrap();

                    field_tokens.extend(quote! {
                        #[wasm_bindgen(getter, js_name = #js_name)]
                        pub fn #get_ident(&self) -> Result<#return_type, wasm_bindgen::JsError> {
                            Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(self.0.#ident.clone(), &bindy::WasmContext)?)
                        }

                        #[wasm_bindgen(setter, js_name = #js_name)]
                        pub fn #set_ident(&mut self, value: #param_type) -> Result<(), wasm_bindgen::JsError> {
                            self.0.#ident = bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(value, &bindy::WasmContext)?;
                            Ok(())
                        }
                    });

                    let stub = apply_mappings_with_flavor(ty, &stubs, MappingFlavor::JavaScript);

                    field_stubs.push_str(&formatdoc! {"
                        {js_name}: {stub};
                    "});
                }

                let mut constructor_stubs = String::new();

                if new {
                    let arg_attrs = fields
                        .keys()
                        .map(|k| {
                            let js_name = k.to_case(Case::Camel);
                            quote!( #[wasm_bindgen(js_name = #js_name)] )
                        })
                        .collect::<Vec<_>>();

                    let arg_idents = fields
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = fields
                        .values()
                        .map(|v| {
                            parse_str::<Type>(apply_mappings(v, &param_mappings).as_str()).unwrap()
                        })
                        .collect::<Vec<_>>();

                    method_tokens.extend(quote! {
                        #[wasm_bindgen(constructor)]
                        pub fn new(
                            #( #arg_attrs #arg_idents: #arg_types ),*
                        ) -> Result<Self, wasm_bindgen::JsError> {
                            Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(#rust_struct_ident {
                                #(#arg_idents: bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)?),*
                            }, &bindy::WasmContext)?)
                        }
                    });

                    let arg_stubs = function_args(&fields, &stubs, MappingFlavor::JavaScript);

                    constructor_stubs.push_str(&formatdoc! {"
                        constructor({arg_stubs});
                    "});
                }

                let option_type_ident =
                    Ident::new(&format!("{name}OptionType"), Span::mixed_site());
                let array_type_ident = Ident::new(&format!("{name}ArrayType"), Span::mixed_site());
                let option_array_type_ident =
                    Ident::new(&format!("{name}OptionArrayType"), Span::mixed_site());

                js_types.extend(quote! {
                    #[wasm_bindgen]
                    pub type #option_type_ident;

                    #[wasm_bindgen]
                    pub type #array_type_ident;

                    #[wasm_bindgen]
                    pub type #option_array_type_ident;
                });

                output.extend(quote! {
                    #[derive(TryFromJsValue)]
                    #[wasm_bindgen(skip_typescript)]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_struct_ident);

                    #[wasm_bindgen]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_struct_ident, T, bindy::Wasm> for #bound_ident {
                        fn from_rust(value: #rust_struct_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_struct_ident, T, bindy::Wasm> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_struct_ident> {
                            Ok(self.0)
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_struct_ident, T, bindy::Wasm> for &'_ #bound_ident {
                        fn into_rust(self, context: &T) -> bindy::Result<#rust_struct_ident> {
                            std::ops::Deref::deref(&self).clone().into_rust(context)
                        }
                    }

                    impl<T> bindy::IntoRust<Option<#rust_struct_ident>, T, bindy::Wasm> for &'_ #option_type_ident {
                        fn into_rust(self, context: &T) -> bindy::Result<Option<#rust_struct_ident>> {
                            let typed_value = try_from_js_option::<#bound_ident>(self).map_err(bindy::Error::Custom)?;
                            typed_value.into_rust(context)
                        }
                    }

                    impl<T> bindy::IntoRust<Vec<#rust_struct_ident>, T, bindy::Wasm> for &'_ #array_type_ident {
                        fn into_rust(self, context: &T) -> bindy::Result<Vec<#rust_struct_ident>> {
                            let typed_value = try_from_js_array::<#bound_ident>(self).map_err(bindy::Error::Custom)?;
                            typed_value.into_rust(context)
                        }
                    }

                    impl<T> bindy::IntoRust<Option<Vec<#rust_struct_ident>>, T, bindy::Wasm> for &'_ #option_array_type_ident {
                        fn into_rust(self, context: &T) -> bindy::Result<Option<Vec<#rust_struct_ident>>> {
                            let typed_value = try_from_js_option_array::<#bound_ident>(self).map_err(bindy::Error::Custom)?;
                            typed_value.into_rust(context)
                        }
                    }
                });

                let body_stubs = format!("{constructor_stubs}{field_stubs}{method_stubs}")
                    .lines()
                    .map(|s| format!("    {s}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                classes.push_str(&formatdoc! {"
                    export class {name} {{
                        free(): void;
                        __getClassname(): string;
                        clone(): {name};
                    {body_stubs}
                    }}
                "});
            }
            Binding::Enum { values } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let value_idents = values
                    .iter()
                    .map(|v| Ident::new(v, Span::mixed_site()))
                    .collect::<Vec<_>>();

                output.extend(quote! {
                    #[wasm_bindgen(skip_typescript)]
                    #[derive(Clone)]
                    pub enum #bound_ident {
                        #( #value_idents ),*
                    }

                    impl<T> bindy::FromRust<#rust_ident, T, bindy::Wasm> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(match value {
                                #( #rust_ident::#value_idents => Self::#value_idents ),*
                            })
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T, bindy::Wasm> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
                            Ok(match self {
                                #( Self::#value_idents => #rust_ident::#value_idents ),*
                            })
                        }
                    }
                });

                let body_stubs = values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("    {v} = {i},"))
                    .collect::<Vec<_>>()
                    .join("\n");

                classes.push_str(&formatdoc! {"
                    export enum {name} {{
                    {body_stubs}
                    }}
                "});
            }
            Binding::Function { args, ret } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let ident = Ident::new(&name, Span::mixed_site());

                let js_name = name.to_case(Case::Camel);

                let arg_attrs = args
                    .keys()
                    .map(|k| {
                        let js_name = k.to_case(Case::Camel);
                        quote!( #[wasm_bindgen(js_name = #js_name)] )
                    })
                    .collect::<Vec<_>>();

                let arg_idents = args
                    .keys()
                    .map(|k| Ident::new(k, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let arg_types = args
                    .values()
                    .map(|v| {
                        parse_str::<Type>(apply_mappings(v, &param_mappings).as_str()).unwrap()
                    })
                    .collect::<Vec<_>>();

                let ret_mapping = parse_str::<Type>(
                    apply_mappings(ret.as_deref().unwrap_or("()"), &return_mappings).as_str(),
                )
                .unwrap();

                output.extend(quote! {
                    #[wasm_bindgen(skip_typescript, js_name = #js_name)]
                    pub fn #bound_ident(
                        #( #arg_attrs #arg_idents: #arg_types ),*
                    ) -> Result<#ret_mapping, wasm_bindgen::JsError> {
                        Ok(bindy::FromRust::<_, _, bindy::Wasm>::from_rust(#entrypoint::#ident(
                            #( bindy::IntoRust::<_, _, bindy::Wasm>::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                        )?, &bindy::WasmContext)?)
                    }
                });

                let arg_stubs = function_args(&args, &stubs, MappingFlavor::JavaScript);

                let ret_stub = apply_mappings_with_flavor(
                    ret.as_deref().unwrap_or("()"),
                    &stubs,
                    MappingFlavor::JavaScript,
                );

                functions.push_str(&formatdoc! {"
                    export function {js_name}({arg_stubs}): {ret_stub};
                "});
            }
        }
    }

    let clvm_type_values = [
        bindy.clvm_types.clone(),
        vec![
            "string | bigint | number | boolean | Uint8Array | null | undefined | ClvmType[]"
                .to_string(),
        ],
    ]
    .concat()
    .join(" | ");
    let clvm_type = format!("export type ClvmType = {clvm_type_values};");

    let typescript = format!("\n{clvm_type}\n\n{functions}\n{classes}");

    output.extend(quote! {
        #[wasm_bindgen]
        extern "C" {
            #js_types
        }

        #[wasm_bindgen(typescript_custom_section)]
        const TS_APPEND_CONTENT: &'static str = #typescript;
    });

    let clvm_types = bindy
        .clvm_types
        .iter()
        .map(|s| Ident::new(s, Span::mixed_site()))
        .collect::<Vec<_>>();

    output.extend(quote! {
        enum ClvmType {
            #( #clvm_types ( #clvm_types ), )*
        }

        fn try_from_js_any(js_val: &JsValue) -> Option<ClvmType> {
            #( if let Ok(value) = #clvm_types::try_from(js_val) {
                return Some(ClvmType::#clvm_types(value));
            } )*

            None
        }
    });

    output.into()
}

#[proc_macro]
pub fn bindy_pyo3(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let (bindy, bindings) = load_bindings(&input);

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut mappings = bindy.pyo3.clone();
    build_base_mappings(&bindy, &mut mappings, &mut IndexMap::new());

    let mut output = quote!();
    let mut module = quote!();

    for (name, binding) in bindings {
        let bound_ident = Ident::new(&name, Span::mixed_site());

        match &binding {
            Binding::Class {
                new,
                remote,
                methods,
                fields,
                no_wasm: _,
            } => {
                let rust_struct_ident = quote!( #entrypoint::#bound_ident );
                let fully_qualified_ident = if *remote {
                    let ext_ident = Ident::new(&format!("{name}Ext"), Span::mixed_site());
                    quote!( <#rust_struct_ident as #entrypoint::#ext_ident> )
                } else {
                    quote!( #rust_struct_ident )
                };

                let mut method_tokens = quote! {
                    pub fn clone(&self) -> Self {
                        Clone::clone(self)
                    }
                };

                for (name, method) in methods {
                    if method.stub_only {
                        // TODO: Add stubs
                        continue;
                    }

                    let method_ident = Ident::new(name, Span::mixed_site());

                    let arg_idents = method
                        .args
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = method
                        .args
                        .values()
                        .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                        .collect::<Vec<_>>();

                    let ret = parse_str::<Type>(
                        apply_mappings(
                            method.ret.as_deref().unwrap_or(
                                if matches!(
                                    method.kind,
                                    MethodKind::Constructor
                                        | MethodKind::Factory
                                        | MethodKind::AsyncFactory
                                ) {
                                    "Self"
                                } else {
                                    "()"
                                },
                            ),
                            &mappings,
                        )
                        .as_str(),
                    )
                    .unwrap();

                    let mut pyo3_attr = match method.kind {
                        MethodKind::Constructor => quote!(#[new]),
                        MethodKind::Static | MethodKind::Factory | MethodKind::AsyncFactory => {
                            quote!(#[staticmethod])
                        }
                        _ => quote!(),
                    };

                    if !matches!(method.kind, MethodKind::ToString) {
                        pyo3_attr = quote! {
                            #pyo3_attr
                            #[pyo3(signature = (#(#arg_idents),*))]
                        };
                    }

                    let remapped_method_ident = if matches!(method.kind, MethodKind::ToString) {
                        Ident::new("__str__", Span::mixed_site())
                    } else {
                        method_ident.clone()
                    };

                    match method.kind {
                        MethodKind::Constructor | MethodKind::Static | MethodKind::Factory => {
                            method_tokens.extend(quote! {
                                #pyo3_attr
                                pub fn #remapped_method_ident(
                                    #( #arg_idents: #arg_types ),*
                                ) -> pyo3::PyResult<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                                    )?, &bindy::Pyo3Context)?)
                                }
                            });
                        }
                        MethodKind::AsyncFactory => {
                            method_tokens.extend(quote! {
                                #pyo3_attr
                                pub async fn #remapped_method_ident(
                                    #( #arg_idents: #arg_types ),*
                                ) -> pyo3::PyResult<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(#fully_qualified_ident::#method_ident(
                                        #( bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                                    ).await?, &bindy::Pyo3Context)?)
                                }
                            });
                        }
                        MethodKind::Normal | MethodKind::ToString => {
                            method_tokens.extend(quote! {
                                #pyo3_attr
                                pub fn #remapped_method_ident(
                                    &self,
                                    #( #arg_idents: #arg_types ),*
                                ) -> pyo3::PyResult<#ret> {
                                    Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(#fully_qualified_ident::#method_ident(
                                        &self.0,
                                        #( bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                                    )?, &bindy::Pyo3Context)?)
                                }
                            });
                        }
                        MethodKind::Async => {
                            method_tokens.extend(quote! {
                                #pyo3_attr
                                pub fn #remapped_method_ident<'a>(
                                    &self,
                                    py: Python<'a>,
                                    #( #arg_idents: #arg_types ),*
                                ) -> pyo3::PyResult<pyo3::Bound<'a, pyo3::PyAny>> {
                                    let clone_of_self = self.0.clone();
                                    #( let #arg_idents = bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)?; )*

                                    pyo3_async_runtimes::tokio::future_into_py(py, async move {
                                        let result: pyo3::PyResult<#ret> = Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(clone_of_self.#method_ident(
                                            #( #arg_idents ),*
                                        ).await?, &bindy::Pyo3Context)?);
                                        result
                                    })
                                }
                            });
                        }
                    }
                }

                let mut field_tokens = quote!();

                for (name, ty) in fields {
                    let ident = Ident::new(name, Span::mixed_site());
                    let get_ident = Ident::new(&format!("get_{name}"), Span::mixed_site());
                    let set_ident = Ident::new(&format!("set_{name}"), Span::mixed_site());
                    let ty = parse_str::<Type>(apply_mappings(ty, &mappings).as_str()).unwrap();

                    field_tokens.extend(quote! {
                        #[getter(#ident)]
                        pub fn #get_ident(&self) -> pyo3::PyResult<#ty> {
                            Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(self.0.#ident.clone(), &bindy::Pyo3Context)?)
                        }

                        #[setter(#ident)]
                        pub fn #set_ident(&mut self, value: #ty) -> pyo3::PyResult<()> {
                            self.0.#ident = bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(value, &bindy::Pyo3Context)?;
                            Ok(())
                        }
                    });
                }

                if *new {
                    let arg_idents = fields
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = fields
                        .values()
                        .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                        .collect::<Vec<_>>();

                    method_tokens.extend(quote! {
                        #[new]
                        #[pyo3(signature = (#(#arg_idents),*))]
                        pub fn new(
                            #( #arg_idents: #arg_types ),*
                        ) -> pyo3::PyResult<Self> {
                            Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(#rust_struct_ident {
                                #(#arg_idents: bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)?),*
                            }, &bindy::Pyo3Context)?)
                        }
                    });
                }

                output.extend(quote! {
                    #[pyo3::pyclass]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_struct_ident);

                    #[pyo3::pymethods]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_struct_ident, T, bindy::Pyo3> for #bound_ident {
                        fn from_rust(value: #rust_struct_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_struct_ident, T, bindy::Pyo3> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_struct_ident> {
                            Ok(self.0)
                        }
                    }
                });
            }
            Binding::Enum { values } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let value_idents = values
                    .iter()
                    .map(|v| Ident::new(v, Span::mixed_site()))
                    .collect::<Vec<_>>();

                output.extend(quote! {
                    #[pyo3::pyclass(eq, eq_int)]
                    #[derive(Clone, PartialEq, Eq)]
                    pub enum #bound_ident {
                        #( #value_idents ),*
                    }

                    impl<T> bindy::FromRust<#rust_ident, T, bindy::Pyo3> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(match value {
                                #( #rust_ident::#value_idents => Self::#value_idents ),*
                            })
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T, bindy::Pyo3> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
                            Ok(match self {
                                #( Self::#value_idents => #rust_ident::#value_idents ),*
                            })
                        }
                    }
                });
            }
            Binding::Function { args, ret } => {
                let arg_idents = args
                    .keys()
                    .map(|k| Ident::new(k, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let arg_types = args
                    .values()
                    .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                    .collect::<Vec<_>>();

                let ret = parse_str::<Type>(
                    apply_mappings(ret.as_deref().unwrap_or("()"), &mappings).as_str(),
                )
                .unwrap();

                output.extend(quote! {
                    #[pyo3::pyfunction]
                    #[pyo3(signature = (#(#arg_idents),*))]
                    pub fn #bound_ident(
                        #( #arg_idents: #arg_types ),*
                    ) -> pyo3::PyResult<#ret> {
                        Ok(bindy::FromRust::<_, _, bindy::Pyo3>::from_rust(#entrypoint::#bound_ident(
                            #( bindy::IntoRust::<_, _, bindy::Pyo3>::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                        )?, &bindy::Pyo3Context)?)
                    }
                });
            }
        }

        match binding {
            Binding::Class { .. } | Binding::Enum { .. } => {
                module.extend(quote! {
                    m.add_class::<#bound_ident>()?;
                });
            }
            Binding::Function { .. } => {
                module.extend(quote! {
                    m.add_function(pyo3::wrap_pyfunction!(#bound_ident, m)?)?;
                });
            }
        }
    }

    let pymodule = Ident::new(&bindy.pymodule, Span::mixed_site());

    output.extend(quote! {
        #[pyo3::pymodule]
        fn #pymodule(m: &pyo3::Bound<'_, pyo3::prelude::PyModule>) -> pyo3::PyResult<()> {
            use pyo3::types::PyModuleMethods;
            #module
            Ok(())
        }
    });

    let clvm_types = bindy
        .clvm_types
        .iter()
        .map(|s| Ident::new(s, Span::mixed_site()))
        .collect::<Vec<_>>();

    output.extend(quote! {
        enum ClvmType {
            #( #clvm_types ( #clvm_types ), )*
        }

        fn extract_clvm_type(value: &Bound<'_, PyAny>) -> Option<ClvmType> {
            #( if let Ok(value) = value.extract::<#clvm_types>() {
                return Some(ClvmType::#clvm_types(value));
            } )*

            None
        }
    });

    output.into()
}

#[proc_macro]
pub fn bindy_pyo3_stubs(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let (bindy, bindings) = load_bindings(&input);

    let mut stubs = bindy.pyo3_stubs.clone();
    build_base_mappings(&bindy, &mut IndexMap::new(), &mut stubs);

    let mut classes = String::new();
    let mut functions = String::new();

    for (name, binding) in bindings {
        match binding {
            Binding::Class {
                new,
                methods,
                fields,
                ..
            } => {
                let mut method_stubs = String::new();

                let class_name = name.clone();

                for (name, method) in methods {
                    let name = if matches!(method.kind, MethodKind::Constructor) {
                        "__init__".to_string()
                    } else {
                        name
                    };

                    let arg_stubs = function_args(&method.args, &stubs, MappingFlavor::Python);

                    let mut ret_stub = apply_mappings_with_flavor(
                        method.ret.as_deref().unwrap_or("()"),
                        &stubs,
                        MappingFlavor::Python,
                    );

                    match method.kind {
                        MethodKind::Async => ret_stub = format!("Awaitable[{ret_stub}]"),
                        MethodKind::Factory => {
                            ret_stub.clone_from(&class_name);
                        }
                        MethodKind::AsyncFactory => {
                            ret_stub = format!("Awaitable[{class_name}]");
                        }
                        _ => {}
                    }

                    let prefix = match method.kind {
                        MethodKind::Factory | MethodKind::Static => "@staticmethod\n",
                        MethodKind::AsyncFactory => "@staticmethod\nasync ",
                        MethodKind::Async => "async ",
                        _ => "",
                    };

                    let self_arg = if matches!(
                        method.kind,
                        MethodKind::Factory | MethodKind::Static | MethodKind::AsyncFactory
                    ) {
                        ""
                    } else if method.args.is_empty() {
                        "self"
                    } else {
                        "self, "
                    };

                    method_stubs.push_str(&formatdoc! {"
                        {prefix}def {name}({self_arg}{arg_stubs}) -> {ret_stub}: ...
                    "});
                }

                let mut field_stubs = String::new();

                for (name, ty) in &fields {
                    let stub = apply_mappings_with_flavor(ty, &stubs, MappingFlavor::Python);

                    field_stubs.push_str(&formatdoc! {"
                        {name}: {stub}
                    "});
                }

                let mut constructor_stubs = String::new();

                if new {
                    let arg_stubs = function_args(&fields, &stubs, MappingFlavor::Python);

                    constructor_stubs.push_str(&formatdoc! {"
                        def __init__(self, {arg_stubs}) -> None: ...
                    "});
                }

                let body_stubs = format!("{constructor_stubs}{field_stubs}{method_stubs}")
                    .lines()
                    .map(|s| format!("    {s}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                classes.push_str(&formatdoc! {"
                    class {name}:
                        def clone(self) -> {name}: ...
                    {body_stubs}
                "});
            }
            Binding::Enum { values } => {
                let body_stubs = values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("    {v} = {i}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                classes.push_str(&formatdoc! {"
                    class {name}(IntEnum):
                    {body_stubs}
                "});
            }
            Binding::Function { args, ret } => {
                let arg_stubs = function_args(&args, &stubs, MappingFlavor::Python);

                let ret_stub = apply_mappings_with_flavor(
                    ret.as_deref().unwrap_or("()"),
                    &stubs,
                    MappingFlavor::Python,
                );

                functions.push_str(&formatdoc! {"
                    def {name}({arg_stubs}) -> {ret_stub}: ...
                "});
            }
        }
    }

    let clvm_type_values = [
        bindy.clvm_types.clone(),
        vec!["str, int, bool, bytes, None, List['ClvmType']".to_string()],
    ]
    .concat()
    .join(", ");
    let clvm_type = format!("ClvmType = Union[{clvm_type_values}]");

    let stubs = format!(
        "from typing import List, Optional, Union, Awaitable\nfrom enum import IntEnum\n\n{clvm_type}\n\n{functions}\n{classes}"
    );

    quote!(#stubs).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MappingFlavor {
    Rust,
    JavaScript,
    Python,
}

fn apply_mappings(ty: &str, mappings: &IndexMap<String, String>) -> String {
    apply_mappings_with_flavor(ty, mappings, MappingFlavor::Rust)
}

fn apply_mappings_with_flavor(
    ty: &str,
    mappings: &IndexMap<String, String>,
    flavor: MappingFlavor,
) -> String {
    // First check if the entire type has a direct mapping
    if let Some(mapped) = mappings.get(ty) {
        return mapped.clone();
    }

    // Check if this is a generic type by looking for < and >
    if let (Some(start), Some(end)) = (ty.find('<'), ty.rfind('>')) {
        let base_type = &ty[..start];
        let generic_part = &ty[start + 1..end];

        // Split generic parameters by comma and trim whitespace
        let generic_params: Vec<&str> = generic_part.split(',').map(str::trim).collect();

        // Recursively apply mappings to each generic parameter
        let mapped_params: Vec<String> = generic_params
            .into_iter()
            .map(|param| apply_mappings_with_flavor(param, mappings, flavor))
            .collect();

        // Check if the base type needs mapping
        let mapped_base = mappings.get(base_type).map_or(base_type, String::as_str);

        // Reconstruct the type with mapped components
        match (flavor, mapped_base) {
            (MappingFlavor::Rust, _) => {
                format!("{}<{}>", mapped_base, mapped_params.join(", "))
            }
            (MappingFlavor::JavaScript, "Option") => {
                format!("{} | undefined", mapped_params[0])
            }
            (MappingFlavor::JavaScript, "Vec") => {
                format!("{}[]", mapped_params[0])
            }
            (MappingFlavor::Python, "Option") => {
                format!("Optional[{}]", mapped_params[0])
            }
            (MappingFlavor::Python, "Vec") => {
                format!("List[{}]", mapped_params[0])
            }
            _ => panic!("Unsupported mapping with flavor {flavor:?} for type {ty}"),
        }
    } else {
        // No generics, return original if no mapping exists
        ty.to_string()
    }
}

fn function_args(
    args: &IndexMap<String, String>,
    stubs: &IndexMap<String, String>,
    mapping_flavor: MappingFlavor,
) -> String {
    let mut has_non_optional = false;
    let mut results = Vec::new();

    for (name, ty) in args.iter().rev() {
        let is_optional = ty.starts_with("Option<");
        let has_default = is_optional && !has_non_optional;
        let ty = apply_mappings_with_flavor(ty, stubs, mapping_flavor);

        results.push(format!(
            "{}{}: {}{}",
            name.to_case(Case::Camel),
            if has_default && matches!(mapping_flavor, MappingFlavor::JavaScript) {
                "?"
            } else {
                ""
            },
            ty,
            if has_default && matches!(mapping_flavor, MappingFlavor::Python) {
                " = None"
            } else {
                ""
            }
        ));

        if !is_optional {
            has_non_optional = true;
        }
    }

    results.reverse();
    results.join(", ")
}
