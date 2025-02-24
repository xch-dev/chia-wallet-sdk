use std::fs;

use convert_case::{Case, Casing};
use indexmap::{indexmap, IndexMap};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde::{Deserialize, Serialize};
use syn::{parse_str, Ident, LitStr, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bindy {
    entrypoint: String,
    pymodule: String,
    bindings: IndexMap<String, Binding>,
    #[serde(default)]
    napi: IndexMap<String, String>,
    #[serde(default)]
    wasm: IndexMap<String, String>,
    #[serde(default)]
    pyo3: IndexMap<String, String>,
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
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MethodKind {
    #[default]
    Normal,
    ToString,
    Static,
    Factory,
    Constructor,
}

#[proc_macro]
pub fn bindy_napi(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let source = fs::read_to_string(input).unwrap();
    let bindy: Bindy = serde_json::from_str(&source).unwrap();

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut base_mappings = indexmap! {
        "()".to_string() => "napi::JsUndefined".to_string(),
        "Vec<u8>".to_string() => "napi::bindgen_prelude::Uint8Array".to_string(),
        "u64".to_string() => "napi::bindgen_prelude::BigInt".to_string(),
        "BigInt".to_string() => "napi::bindgen_prelude::BigInt".to_string(),
    };
    base_mappings.extend(bindy.napi);

    let mut param_mappings = base_mappings.clone();
    let return_mappings = base_mappings;

    for (name, binding) in &bindy.bindings {
        if matches!(binding, Binding::Class { .. }) {
            param_mappings.insert(
                name.clone(),
                format!("napi::bindgen_prelude::ClassInstance<'_, {name}>"),
            );
        }
    }

    let mut output = quote!();

    for (name, binding) in bindy.bindings {
        match binding {
            Binding::Class {
                new,
                methods,
                fields,
            } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let mut method_tokens = quote!();

                for (name, method) in methods {
                    let method_ident = Ident::new(&name, Span::mixed_site());

                    let arg_idents = method
                        .args
                        .keys()
                        .map(|k| Ident::new(k, Span::mixed_site()))
                        .collect::<Vec<_>>();

                    let arg_types = method
                        .args
                        .values()
                        .map(|v| {
                            parse_str::<Type>(apply_mappings(v, &param_mappings).as_str()).unwrap()
                        })
                        .collect::<Vec<_>>();

                    let ret = parse_str::<Type>(
                        apply_mappings(
                            method.ret.as_deref().unwrap_or(
                                if matches!(
                                    method.kind,
                                    MethodKind::Constructor | MethodKind::Factory
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
                        MethodKind::Factory => quote!(#[napi(factory)]),
                        MethodKind::Normal | MethodKind::ToString => quote!(#[napi]),
                    };

                    match method.kind {
                        MethodKind::Constructor | MethodKind::Static | MethodKind::Factory => {
                            method_tokens.extend(quote! {
                                #napi_attr
                                pub fn #method_ident(
                                    env: Env,
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<#ret> {
                                    Ok(bindy::FromRust::from_rust(#rust_ident::#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    )?, &bindy::NapiReturnContext(env))?)
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
                                    Ok(bindy::FromRust::from_rust(self.0.#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                                    )?, &bindy::NapiReturnContext(env))?)
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
                        parse_str::<Type>(apply_mappings(ty, &param_mappings).as_str()).unwrap();

                    field_tokens.extend(quote! {
                        #[napi(getter)]
                        pub fn #get_ident(&self, env: Env) -> napi::Result<#get_ty> {
                            Ok(bindy::FromRust::from_rust(self.0.#ident.clone(), &bindy::NapiReturnContext(env))?)
                        }

                        #[napi(setter)]
                        pub fn #set_ident(&mut self, env: Env, value: #set_ty) -> napi::Result<()> {
                            self.0.#ident = bindy::IntoRust::into_rust(value, &bindy::NapiParamContext)?;
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
                            parse_str::<Type>(apply_mappings(v, &param_mappings).as_str()).unwrap()
                        })
                        .collect::<Vec<_>>();

                    method_tokens.extend(quote! {
                        #[napi(constructor)]
                        pub fn new(
                            env: Env,
                            #( #arg_idents: #arg_types ),*
                        ) -> napi::Result<Self> {
                            Ok(bindy::FromRust::from_rust(#rust_ident {
                                #(#arg_idents: bindy::IntoRust::into_rust(#arg_idents, &bindy::NapiParamContext)?),*
                            }, &bindy::NapiReturnContext(env))?)
                        }
                    });
                }

                output.extend(quote! {
                    #[napi_derive::napi]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_ident);

                    #[napi_derive::napi]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_ident, T> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
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
                    #[derive(Clone)]
                    pub enum #bound_ident {
                        #( #value_idents ),*
                    }

                    impl<T> bindy::FromRust<#rust_ident, T> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(match value {
                                #( #rust_ident::#value_idents => Self::#value_idents ),*
                            })
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T> for #bound_ident {
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
                        parse_str::<Type>(apply_mappings(v, &param_mappings).as_str()).unwrap()
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
                        Ok(bindy::FromRust::from_rust(#entrypoint::#ident(
                            #( bindy::IntoRust::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                        )?, &bindy::NapiReturnContext(env))?)
                    }
                });
            }
        }
    }

    output.into()
}

#[proc_macro]
pub fn bindy_wasm(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let source = fs::read_to_string(input).unwrap();
    let bindy: Bindy = serde_json::from_str(&source).unwrap();

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut mappings = indexmap! {
        "u64".to_string() => "js_sys::BigInt".to_string(),
        "BigInt".to_string() => "js_sys::BigInt".to_string(),
    };
    mappings.extend(bindy.wasm);

    let mut output = quote!();

    for (name, binding) in bindy.bindings {
        match binding {
            Binding::Class {
                new,
                methods,
                fields,
            } => {
                let bound_ident = Ident::new(&name, Span::mixed_site());
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let mut method_tokens = quote!();

                for (name, method) in methods {
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
                        .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                        .collect::<Vec<_>>();

                    let ret = parse_str::<Type>(
                        apply_mappings(
                            method.ret.as_deref().unwrap_or(
                                if matches!(
                                    method.kind,
                                    MethodKind::Constructor | MethodKind::Factory
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

                    let wasm_attr = match method.kind {
                        MethodKind::Constructor => quote!(#[wasm_bindgen(constructor)]),
                        _ => quote!(#[wasm_bindgen(js_name = #js_name)]),
                    };

                    match method.kind {
                        MethodKind::Constructor | MethodKind::Static | MethodKind::Factory => {
                            method_tokens.extend(quote! {
                                #wasm_attr
                                pub fn #method_ident(
                                    #( #arg_attrs #arg_idents: #arg_types ),*
                                ) -> Result<#ret, wasm_bindgen::JsError> {
                                    Ok(bindy::FromRust::from_rust(#rust_ident::#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    )?, &bindy::WasmContext)?)
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
                                    Ok(bindy::FromRust::from_rust(self.0.#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                                    )?, &bindy::WasmContext)?)
                                }
                            });
                        }
                    }
                }

                let mut field_tokens = quote!();

                for (name, ty) in &fields {
                    let js_name = name.to_case(Case::Camel);
                    let ident = Ident::new(name, Span::mixed_site());
                    let get_ident = Ident::new(&format!("get_{name}"), Span::mixed_site());
                    let set_ident = Ident::new(&format!("set_{name}"), Span::mixed_site());
                    let ty = parse_str::<Type>(apply_mappings(ty, &mappings).as_str()).unwrap();

                    field_tokens.extend(quote! {
                        #[wasm_bindgen(getter, js_name = #js_name)]
                        pub fn #get_ident(&self) -> Result<#ty, wasm_bindgen::JsError> {
                            Ok(bindy::FromRust::from_rust(self.0.#ident.clone(), &bindy::WasmContext)?)
                        }

                        #[wasm_bindgen(setter, js_name = #js_name)]
                        pub fn #set_ident(&mut self, value: #ty) -> Result<(), wasm_bindgen::JsError> {
                            self.0.#ident = bindy::IntoRust::into_rust(value, &bindy::WasmContext)?;
                            Ok(())
                        }
                    });
                }

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
                        .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                        .collect::<Vec<_>>();

                    method_tokens.extend(quote! {
                        #[wasm_bindgen(constructor)]
                        pub fn new(
                            #( #arg_attrs #arg_idents: #arg_types ),*
                        ) -> Result<Self, wasm_bindgen::JsError> {
                            Ok(bindy::FromRust::from_rust(#rust_ident {
                                #(#arg_idents: bindy::IntoRust::into_rust(#arg_idents, &bindy::WasmContext)?),*
                            }, &bindy::WasmContext)?)
                        }
                    });
                }

                output.extend(quote! {
                    #[wasm_bindgen::prelude::wasm_bindgen]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_ident);

                    #[wasm_bindgen::prelude::wasm_bindgen]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_ident, T> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
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
                    #[wasm_bindgen::prelude::wasm_bindgen]
                    #[derive(Clone)]
                    pub enum #bound_ident {
                        #( #value_idents ),*
                    }

                    impl<T> bindy::FromRust<#rust_ident, T> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(match value {
                                #( #rust_ident::#value_idents => Self::#value_idents ),*
                            })
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T> for #bound_ident {
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
                    .map(|v| parse_str::<Type>(apply_mappings(v, &mappings).as_str()).unwrap())
                    .collect::<Vec<_>>();

                let ret = parse_str::<Type>(
                    apply_mappings(ret.as_deref().unwrap_or("()"), &mappings).as_str(),
                )
                .unwrap();

                output.extend(quote! {
                    #[wasm_bindgen::prelude::wasm_bindgen(js_name = #js_name)]
                    pub fn #bound_ident(
                        #( #arg_attrs #arg_idents: #arg_types ),*
                    ) -> Result<#ret, wasm_bindgen::JsError> {
                        Ok(bindy::FromRust::from_rust(#entrypoint::#ident(
                            #( bindy::IntoRust::into_rust(#arg_idents, &bindy::WasmContext)? ),*
                        )?, &bindy::WasmContext)?)
                    }
                });
            }
        }
    }

    output.into()
}

#[proc_macro]
pub fn bindy_pyo3(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let source = fs::read_to_string(input).unwrap();
    let bindy: Bindy = serde_json::from_str(&source).unwrap();

    let entrypoint = Ident::new(&bindy.entrypoint, Span::mixed_site());

    let mut mappings = indexmap! {
        "BigInt".to_string() => "num_bigint::BigInt".to_string(),
    };
    mappings.extend(bindy.pyo3);

    let mut output = quote!();
    let mut module = quote!();

    for (name, binding) in bindy.bindings {
        let bound_ident = Ident::new(&name, Span::mixed_site());

        match &binding {
            Binding::Class {
                new,
                methods,
                fields,
            } => {
                let rust_ident = quote!( #entrypoint::#bound_ident );

                let mut method_tokens = quote!();

                for (name, method) in methods {
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
                                    MethodKind::Constructor | MethodKind::Factory
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
                        MethodKind::Static => quote!(#[staticmethod]),
                        MethodKind::Factory => quote!(#[staticmethod]),
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
                                    Ok(bindy::FromRust::from_rust(#rust_ident::#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                                    )?, &bindy::Pyo3Context)?)
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
                                    Ok(bindy::FromRust::from_rust(self.0.#method_ident(
                                        #( bindy::IntoRust::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                                    )?, &bindy::Pyo3Context)?)
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
                            Ok(bindy::FromRust::from_rust(self.0.#ident.clone(), &bindy::Pyo3Context)?)
                        }

                        #[setter(#ident)]
                        pub fn #set_ident(&mut self, value: #ty) -> pyo3::PyResult<()> {
                            self.0.#ident = bindy::IntoRust::into_rust(value, &bindy::Pyo3Context)?;
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
                            Ok(bindy::FromRust::from_rust(#rust_ident {
                                #(#arg_idents: bindy::IntoRust::into_rust(#arg_idents, &bindy::Pyo3Context)?),*
                            }, &bindy::Pyo3Context)?)
                        }
                    });
                }

                output.extend(quote! {
                    #[pyo3::pyclass]
                    #[derive(Clone)]
                    pub struct #bound_ident(#rust_ident);

                    #[pyo3::pymethods]
                    impl #bound_ident {
                        #method_tokens
                        #field_tokens
                    }

                    impl<T> bindy::FromRust<#rust_ident, T> for #bound_ident {
                        fn from_rust(value: #rust_ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#rust_ident, T> for #bound_ident {
                        fn into_rust(self, _context: &T) -> bindy::Result<#rust_ident> {
                            Ok(self.0)
                        }
                    }
                });
            }
            Binding::Enum { values } => {
                todo!()
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
                    pub fn #bound_ident(
                        #( #arg_idents: #arg_types ),*
                    ) -> pyo3::PyResult<#ret> {
                        Ok(bindy::FromRust::from_rust(#entrypoint::#bound_ident(
                            #( bindy::IntoRust::into_rust(#arg_idents, &bindy::Pyo3Context)? ),*
                        )?, &bindy::Pyo3Context)?)
                    }
                });
            }
        }

        match binding {
            Binding::Class { .. } => {
                module.extend(quote! {
                    m.add_class::<#bound_ident>()?;
                });
            }
            Binding::Enum { values } => {
                todo!()
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

    output.into()
}

fn apply_mappings(ty: &str, mappings: &IndexMap<String, String>) -> String {
    // First check if the entire type has a direct mapping
    if let Some(mapped) = mappings.get(ty) {
        return mapped.clone();
    }

    // Check if this is a generic type by looking for < and >
    if let (Some(start), Some(end)) = (ty.find('<'), ty.rfind('>')) {
        let base_type = &ty[..start];
        let generic_part = &ty[start + 1..end];

        // Split generic parameters by comma and trim whitespace
        let generic_params: Vec<&str> = generic_part.split(',').map(|s| s.trim()).collect();

        // Recursively apply mappings to each generic parameter
        let mapped_params: Vec<String> = generic_params
            .into_iter()
            .map(|param| apply_mappings(param, mappings))
            .collect();

        // Check if the base type needs mapping
        let mapped_base = mappings
            .get(base_type)
            .map(|s| s.as_str())
            .unwrap_or(base_type);

        // Reconstruct the type with mapped components
        format!("{}<{}>", mapped_base, mapped_params.join(", "))
    } else {
        // No generics, return original if no mapping exists
        ty.to_string()
    }
}
