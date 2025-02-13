mod binding;

use binding::{bindings, BindingType};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, Ident, LitStr};

fn napi_type(ty: &str) -> String {
    match ty {
        "String" => "string".to_string(),
        "Bytes" | "Bytes32" => "Uint8Array".to_string(),
        _ => panic!("Unsupported type for NAPI typings: {ty}"),
    }
}

#[proc_macro]
pub fn include_napi_bindings(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as LitStr).value();
    let bindings = bindings(&root);

    let mut tokens = proc_macro2::TokenStream::new();

    for binding in bindings {
        let name = Ident::new(&binding.name, Span::mixed_site());

        let BindingType::Function { args, returns } = binding.kind;

        let param_names = args
            .iter()
            .map(|arg| Ident::new(&arg.name, Span::mixed_site()))
            .collect::<Vec<_>>();

        let param_types = args
            .iter()
            .map(|arg| Ident::new(&arg.ty, Span::mixed_site()))
            .collect::<Vec<_>>();

        let napi_types = args
            .iter()
            .map(|arg| napi_type(&arg.ty))
            .collect::<Vec<_>>();

        let napi_returns = napi_type(&returns);
        let returns = Ident::new(&returns, Span::mixed_site());

        let napi_fn = quote! {
            #[napi_derive::napi(ts_return_type = #napi_returns)]
            pub fn #name( #( #[napi(ts_arg_type = #napi_types)] #param_names: <#param_types as Unbind>::Bound),* ) -> napi::Result< <#returns as Unbind>::Bound > {
                #(let #param_names = <#param_types as Unbind>::unbind(#param_names)?;)*
                Ok(Bind::bind(chia_sdk_bindings::#name(#(#param_names),*)?)?)
            }

        };

        tokens.extend(napi_fn);
    }

    tokens.into()
}

fn wasm_type(ty: &str) -> String {
    match ty {
        "String" => "string".to_string(),
        "Bytes" | "Bytes32" => "Uint8Array".to_string(),
        _ => panic!("Unsupported type for NAPI typings: {ty}"),
    }
}

#[proc_macro]
pub fn include_wasm_bindings(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as LitStr).value();
    let bindings = bindings(&root);

    let mut tokens = proc_macro2::TokenStream::new();

    for binding in bindings {
        let name = Ident::new(&binding.name, Span::mixed_site());

        let BindingType::Function { args, returns } = binding.kind;

        let param_names = args
            .iter()
            .map(|arg| Ident::new(&arg.name, Span::mixed_site()))
            .collect::<Vec<_>>();

        let param_types = args
            .iter()
            .map(|arg| Ident::new(&arg.ty, Span::mixed_site()))
            .collect::<Vec<_>>();

        let napi_types = args
            .iter()
            .map(|arg| napi_type(&arg.ty))
            .collect::<Vec<_>>();

        let napi_returns = napi_type(&returns);
        let returns = Ident::new(&returns, Span::mixed_site());

        let napi_fn = quote! {
            #[napi_derive::napi(ts_return_type = #napi_returns)]
            pub fn #name( #( #[napi(ts_arg_type = #napi_types)] #param_names: <#param_types as Unbind>::Bound),* ) -> napi::Result< <#returns as Unbind>::Bound > {
                #(let #param_names = <#param_types as Unbind>::unbind(#param_names)?;)*
                Ok(Bind::bind(chia_sdk_bindings::#name(#(#param_names),*)?)?)
            }

        };

        tokens.extend(napi_fn);
    }

    tokens.into()
}
