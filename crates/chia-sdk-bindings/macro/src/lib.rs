mod binding;

use binding::{bindings, BindingType};
use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, parse_str, Ident, LitStr, Type};

fn shared() -> proc_macro2::TokenStream {
    quote! {
        use chia_sdk_bindings::prelude::*;

        pub trait Bind<T> {
            fn bind(self) -> Result<T>;
        }

        impl Bind<String> for String {
            fn bind(self) -> Result<String> {
                Ok(self)
            }
        }

        impl Bind<bool> for bool {
            fn bind(self) -> Result<bool> {
                Ok(self)
            }
        }

        pub trait Unbind: Sized {
            type Bound;

            fn unbind(value: Self::Bound) -> Result<Self>;
        }

        impl Unbind for String {
            type Bound = String;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(value)
            }
        }

        impl Unbind for bool {
            type Bound = bool;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(value)
            }
        }
    }
}

fn napi_type(ty: &str) -> String {
    if let Some(ty) = ty.strip_prefix("rust::") {
        return ty.to_string();
    }

    match ty {
        "String" => "string".to_string(),
        "Bytes" | "Bytes32" => "Uint8Array".to_string(),
        "bool" => "boolean".to_string(),
        "usize" => "number".to_string(),
        _ => panic!("Unsupported type for NAPI typings: {ty}"),
    }
}

#[proc_macro]
pub fn include_napi_bindings(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as LitStr).value();
    let bindings = bindings(&root);
    let shared = shared();

    let mut tokens = quote! {
        #shared

        use napi::bindgen_prelude::Uint8Array;

        impl Bind<Uint8Array> for Bytes {
            fn bind(self) -> Result<Uint8Array> {
                Ok(Uint8Array::from(self.as_ref()))
            }
        }

        impl<const N: usize> Bind<Uint8Array> for BytesImpl<N> {
            fn bind(self) -> Result<Uint8Array> {
                Ok(Uint8Array::from(self.as_ref()))
            }
        }

        impl Bind<i64> for usize {
            fn bind(self) -> Result<i64> {
                Ok(self as i64)
            }
        }

        impl Unbind for Bytes {
            type Bound = Uint8Array;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(Bytes::new(value.to_vec()))
            }
        }

        impl<const N: usize> Unbind for BytesImpl<N> {
            type Bound = Uint8Array;

            fn unbind(value: Self::Bound) -> Result<Self> {
                let bytes = value.as_ref();

                if bytes.len() != N {
                    return Err(Error::WrongLength {
                        expected: N,
                        found: bytes.len(),
                    });
                }

                Ok(BytesImpl::new(bytes.try_into().unwrap()))
            }
        }

        impl Unbind for usize {
            type Bound = i64;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(value as usize)
            }
        }
    };

    for binding in bindings {
        let name = Ident::new(&binding.name, Span::mixed_site());

        match binding.kind {
            BindingType::Function { args, returns } => {
                let param_names = args
                    .iter()
                    .map(|arg| Ident::new(arg.0, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let param_types = args
                    .iter()
                    .map(|arg| parse_str::<Type>(arg.1).unwrap())
                    .collect::<Vec<_>>();

                let napi_types = args.iter().map(|arg| napi_type(arg.1)).collect::<Vec<_>>();

                let napi_returns = napi_type(&returns);
                let returns = parse_str::<Type>(&returns).unwrap();

                let napi_fn = quote! {
                    #[napi_derive::napi(ts_return_type = #napi_returns)]
                    pub fn #name( #( #[napi(ts_arg_type = #napi_types)] #param_names: <#param_types as Unbind>::Bound),* ) -> napi::Result< <#returns as Unbind>::Bound > {
                        #(let #param_names = <#param_types as Unbind>::unbind(#param_names)?;)*
                        Ok(Bind::bind(chia_sdk_bindings::#name(#(#param_names),*)?)?)
                    }
                };

                tokens.extend(napi_fn);
            }
            BindingType::Struct { fields } => {
                let field_names = fields
                    .iter()
                    .map(|arg| Ident::new(arg.0, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let field_types = fields
                    .iter()
                    .map(|arg| parse_str::<Type>(arg.1).unwrap())
                    .collect::<Vec<_>>();

                let napi_field_names = fields
                    .iter()
                    .map(|arg| arg.0.to_case(Case::Camel))
                    .collect::<Vec<_>>();

                let napi_field_types = fields
                    .iter()
                    .map(|arg| napi_type(arg.1))
                    .collect::<Vec<_>>();

                let napi_struct = quote! {
                    #[napi_derive::napi(object)]
                    pub struct #name {
                        #(
                            #[napi(js_name = #napi_field_names, ts_type = #napi_field_types)]
                            pub #field_names: <#field_types as Unbind>::Bound
                        ),*
                    }

                    impl Unbind for chia_sdk_bindings::#name {
                        type Bound = #name;

                        fn unbind(value: Self::Bound) -> Result<Self> {
                            Ok(Self {
                                #(
                                    #field_names: <#field_types as Unbind>::unbind(value.#field_names)?,
                                )*
                            })
                        }
                    }

                    impl Bind<#name> for chia_sdk_bindings::#name {
                        fn bind(self) -> Result<#name> {
                            Ok(#name {
                                #(
                                    #field_names: Bind::bind(self.#field_names)?,
                                )*
                            })
                        }
                    }
                };

                tokens.extend(napi_struct);
            }
        }
    }

    tokens.into()
}

#[proc_macro]
pub fn include_wasm_bindings(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as LitStr).value();
    let bindings = bindings(&root);
    let shared = shared();

    let mut tokens = quote! {
        #shared

        impl Bind<Vec<u8>> for Bytes {
            fn bind(self) -> Result<Vec<u8>> {
                Ok(self.into_inner())
            }
        }

        impl<const N: usize> Bind<Vec<u8>> for BytesImpl<N> {
            fn bind(self) -> Result<Vec<u8>> {
                Ok(self.to_vec())
            }
        }

        impl Bind<usize> for usize {
            fn bind(self) -> Result<usize> {
                Ok(self)
            }
        }

        impl Unbind for Bytes {
            type Bound = Vec<u8>;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(Bytes::new(value))
            }
        }

        impl<const N: usize> Unbind for BytesImpl<N> {
            type Bound = Vec<u8>;

            fn unbind(value: Self::Bound) -> Result<Self> {
                if value.len() != N {
                    return Err(Error::WrongLength {
                        expected: N,
                        found: value.len(),
                    });
                }

                Ok(BytesImpl::new(value.try_into().unwrap()))
            }
        }

        impl Unbind for usize {
            type Bound = usize;

            fn unbind(value: Self::Bound) -> Result<Self> {
                Ok(value)
            }
        }
    };

    for binding in bindings {
        let name = Ident::new(&binding.name, Span::mixed_site());

        match binding.kind {
            BindingType::Function { args, returns } => {
                let camel_name = binding.name.to_case(Case::Camel);

                let param_names = args
                    .iter()
                    .map(|arg| Ident::new(arg.0, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let param_types = args
                    .iter()
                    .map(|arg| parse_str::<Type>(arg.1).unwrap())
                    .collect::<Vec<_>>();

                let wasm_param_names = args
                    .iter()
                    .map(|arg| arg.0.to_case(Case::Camel))
                    .collect::<Vec<_>>();

                let returns = parse_str::<Type>(&returns).unwrap();

                let wasm_fn = quote! {
                    #[wasm_bindgen::prelude::wasm_bindgen(js_name = #camel_name)]
                    pub fn #name( #( #[wasm_bindgen(js_name = #wasm_param_names)] #param_names: <#param_types as Unbind>::Bound),* ) -> std::result::Result< <#returns as Unbind>::Bound, wasm_bindgen::JsError > {
                        #(let #param_names = <#param_types as Unbind>::unbind(#param_names)?;)*
                        Ok(Bind::bind(chia_sdk_bindings::#name(#(#param_names),*)?)?)
                    }
                };

                tokens.extend(wasm_fn);
            }
            BindingType::Struct { fields } => {
                let name_string = binding.name;

                let field_names = fields
                    .iter()
                    .map(|arg| Ident::new(arg.0, Span::mixed_site()))
                    .collect::<Vec<_>>();

                let field_types = fields
                    .iter()
                    .map(|arg| parse_str::<Type>(arg.1).unwrap())
                    .collect::<Vec<_>>();

                let wasm_field_names = fields
                    .iter()
                    .map(|arg| arg.0.to_case(Case::Camel))
                    .collect::<Vec<_>>();

                let wasm_struct = quote! {
                    #[wasm_bindgen::prelude::wasm_bindgen(js_name = #name_string)]
                    pub struct #name {
                        #(
                            #[wasm_bindgen(js_name = #wasm_field_names, getter_with_clone, setter)]
                            pub #field_names: <#field_types as Unbind>::Bound
                        ),*
                    }

                    #[wasm_bindgen::prelude::wasm_bindgen]
                    impl #name {
                        #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
                        pub fn new( #( #[wasm_bindgen(js_name = #wasm_field_names)] #field_names: <#field_types as Unbind>::Bound),* ) -> Self {
                            Self { #( #field_names, )* }
                        }
                    }

                    impl Unbind for chia_sdk_bindings::#name {
                        type Bound = #name;

                        fn unbind(value: Self::Bound) -> Result<Self> {
                            Ok(Self {
                                #(
                                    #field_names: <#field_types as Unbind>::unbind(value.#field_names)?,
                                )*
                            })
                        }
                    }

                    impl Bind<#name> for chia_sdk_bindings::#name {
                        fn bind(self) -> Result<#name> {
                            Ok(#name {
                                #(
                                    #field_names: Bind::bind(self.#field_names)?,
                                )*
                            })
                        }
                    }
                };

                tokens.extend(wasm_struct);
            }
        }
    }

    tokens.into()
}
