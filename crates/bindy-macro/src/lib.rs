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
    bindings: IndexMap<String, Binding>,
    #[serde(default)]
    napi: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Binding {
    Class {
        methods: IndexMap<String, Method>,
    },
    Function {
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
    Constructor,
}

#[proc_macro]
pub fn bindy_napi(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as LitStr).value();
    let source = fs::read_to_string(input).unwrap();
    let bindy: Bindy = serde_json::from_str(&source).unwrap();

    let mut base_mappings = indexmap! {
        "()".to_string() => "napi::JsUndefined".to_string(),
        "Vec<u8>".to_string() => "napi::bindgen_prelude::Uint8Array".to_string(),
    };
    base_mappings.extend(bindy.napi);

    let mut param_mappings = base_mappings.clone();
    let mut return_mappings = base_mappings.clone();

    for (name, binding) in &bindy.bindings {
        if matches!(binding, Binding::Class { .. }) {
            param_mappings.insert(
                name.clone(),
                format!("napi::bindgen_prelude::ClassInstance<'a, {name}Bound>"),
            );
            return_mappings.insert(name.clone(), format!("{name}Bound"));
        }
    }

    let mut output = quote!();

    for (name, binding) in bindy.bindings {
        match binding {
            Binding::Class { methods } => {
                let ident = Ident::new(&name, Span::mixed_site());
                let bound_name = Ident::new(&format!("{name}Bound"), Span::mixed_site());

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
                        apply_mappings(method.ret.as_deref().unwrap_or("()"), &return_mappings)
                            .as_str(),
                    )
                    .unwrap();

                    match method.kind {
                        MethodKind::Constructor => {
                            method_tokens.extend(quote! {
                                #[napi(constructor)]
                                pub fn #method_ident<'a>(
                                    env: Env,
                                    #( #arg_idents: #arg_types ),*
                                ) -> napi::Result<Self> {
                                    Ok(bindy::FromRust::from_rust(#ident::#method_ident(
                                        #(#arg_idents.into_rust(&bindy::NapiParamContext)?),*
                                    )?, &bindy::NapiReturnContext(env))?)
                                }
                            });
                        }
                        MethodKind::Normal => {
                            method_tokens.extend(quote! {
                                #[napi]
                                pub fn #method_ident<'a>(
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

                output.extend(quote! {
                    #[napi_derive::napi(js_name = #name)]
                    #[derive(Clone)]
                    pub struct #bound_name(#ident);

                    #[napi_derive::napi]
                    impl #bound_name {
                        #method_tokens
                    }

                    impl<T> bindy::FromRust<#ident, T> for #bound_name {
                        fn from_rust(value: #ident, _context: &T) -> bindy::Result<Self> {
                            Ok(Self(value))
                        }
                    }

                    impl<T> bindy::IntoRust<#ident, T> for #bound_name {
                        fn into_rust(self, _context: &T) -> bindy::Result<#ident> {
                            Ok(self.0)
                        }
                    }
                });
            }
            Binding::Function { args, ret } => {
                let bound_ident = Ident::new(&format!("{name}_bound"), Span::mixed_site());
                let ident = Ident::new(&name, Span::mixed_site());

                let js_name = name.to_case(Case::Camel);

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
                    #[napi_derive::napi(js_name = #js_name)]
                    pub fn #bound_ident<'a>(
                        env: Env,
                        #( #arg_idents: #arg_types ),*
                    ) -> napi::Result<#ret> {
                        Ok(bindy::FromRust::from_rust(#ident(
                            #( bindy::IntoRust::into_rust(#arg_idents, &bindy::NapiParamContext)? ),*
                        )?, &bindy::NapiReturnContext(env))?)
                    }
                });
            }
        }
    }

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
