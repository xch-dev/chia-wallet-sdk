use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, FnArg, GenericArgument, Ident, ItemFn, Pat, PatType, PathArguments,
    ReturnType, Type,
};

#[proc_macro_attribute]
pub fn bind(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let vis = &input_fn.vis;
    let name = &input_fn.sig.ident;
    let output = &input_fn.sig.output;
    let body = &input_fn.block;

    // Generate impl function name
    let impl_name = Ident::new(&format!("{name}_impl"), name.span());

    // Process arguments
    let (param_names, param_types): (Vec<_>, Vec<_>) = input_fn
        .sig
        .inputs
        .iter()
        .map(|arg| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if let Pat::Ident(pat_ident) = &**pat {
                    (pat_ident.ident.clone(), (*ty).clone())
                } else {
                    panic!("Unsupported parameter pattern")
                }
            } else {
                panic!("Unsupported parameter type")
            }
        })
        .unzip();

    // Generate the wrapped function
    let ReturnType::Type(_, ret) = output else {
        panic!("Functions without return types are not supported");
    };

    let inner_type = extract_result_type(ret);

    let name_string = name.to_string().to_case(Case::Camel);

    let param_name_strings = param_names
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let param_types_unbound = param_types
        .iter()
        .map(|ty| quote!( <#ty as crate::Unbind>::Bound ))
        .collect::<Vec<_>>();

    let mut param_strings = Vec::new();

    for (name, ty) in param_name_strings
        .into_iter()
        .zip(param_types_unbound.into_iter())
    {
        let name = name.to_case(Case::Camel);

        param_strings.push(quote! {
            format!("{}: {}", #name, <#ty as crate::Typings>::typings())
        });
    }

    let params = quote!( #(#param_names: <#param_types as crate::Unbind>::Bound),* );

    let wrapped_fn = quote! {
        #[napi_derive::napi]
        #vis fn #name(#params) -> napi::Result<#inner_type> {
            #(let #param_names = <#param_types as crate::Unbind>::unbind(#param_names)?;)*
            Ok(crate::Bind::bind(#impl_name(#(#param_names),*)?)?)
        }

        inventory::submit! {
            crate::binding::TypedFn::new(#name_string, || {
                let result = vec![ #(#param_strings),* ].join(", ");
                format!("export declare function {}({}): {}", #name_string, result, <#inner_type as crate::Typings>::typings())
            })
        }
    };

    // Generate the implementation function
    let impl_fn = quote! {
        fn #impl_name(#(#param_names: #param_types),*) #output {
            #body
        }
    };

    // Combine both functions
    let output = quote! {
        #wrapped_fn
        #impl_fn
    };

    output.into()
}

fn extract_result_type(ty: &Type) -> &Type {
    let Type::Path(type_path) = ty else {
        panic!("Expected a Result type");
    };

    let Some(segment) = type_path.path.segments.last() else {
        panic!("Expected a Result type");
    };

    assert!(
        segment.ident == "Result",
        "Expected a Result type, got {:?}",
        segment.ident
    );

    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        panic!("Expected a Result type");
    };

    let Some(GenericArgument::Type(ty)) = args.args.first() else {
        panic!("Expected a Result type");
    };

    ty
}
