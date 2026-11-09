use convert_case::{Case, Casing};
use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Expr, Ident, Token, Type, braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::{Pair, Punctuated},
    visit_mut::VisitMut,
};

#[derive(Debug, Clone)]
struct Conditions {
    attrs: Vec<Attribute>,
    name: Ident,
    generic: Ident,
    items: Punctuated<Condition, Token![,]>,
}

#[derive(Debug, Clone)]
struct Condition {
    attrs: Vec<Attribute>,
    name: Ident,
    generics: Option<Punctuated<Ident, Token![,]>>,
    additional_derives: Option<AdditionalDerives>,
    fields: Punctuated<ConditionField, Token![,]>,
}

#[derive(Debug, Clone)]
struct AdditionalDerives {
    derives: Punctuated<Ident, Token![+]>,
}

#[derive(Debug, Clone)]
struct ConditionField {
    attrs: Vec<Attribute>,
    modifier: Modifier,
    name: Ident,
    ty: Type,
    constant: Option<Expr>,
}

#[derive(Debug, Clone)]
enum Modifier {
    None,
    Spread,
    Optional,
}

impl Parse for Conditions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        input.parse::<Token![pub]>()?;
        input.parse::<Token![enum]>()?;
        let name = input.parse()?;
        input.parse::<Token![<]>()?;
        let generic = input.parse()?;
        input.parse::<Token![>]>()?;

        let content;
        braced!(content in input);
        let items = content.parse_terminated(Condition::parse, Token![,])?;

        Ok(Self {
            attrs,
            name,
            generic,
            items,
        })
    }
}

impl Parse for Condition {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let name = input.parse()?;
        let generics = if input.peek(Token![<]) {
            input.parse::<Token![<]>()?;
            let mut generics = Punctuated::new();
            loop {
                generics.push_value(input.parse()?);
                if input.peek(Token![>]) {
                    break;
                }
                generics.push_punct(input.parse()?);
            }
            input.parse::<Token![>]>()?;
            Some(generics)
        } else {
            None
        };
        let additional_derives = if input.peek(Token![as]) {
            Some(input.parse()?)
        } else {
            None
        };

        let content;
        braced!(content in input);
        let fields = content.parse_terminated(ConditionField::parse, Token![,])?;

        Ok(Self {
            attrs,
            name,
            generics,
            additional_derives,
            fields,
        })
    }
}

impl Parse for AdditionalDerives {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![as]>()?;

        let mut derives = Punctuated::new();

        loop {
            derives.push_value(input.parse()?);

            if input.peek(Token![+]) {
                derives.push_punct(input.parse()?);
            } else {
                break;
            }
        }

        Ok(Self { derives })
    }
}

impl Parse for ConditionField {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let mut modifier = Modifier::None;

        if input.peek(Token![...]) {
            input.parse::<Token![...]>()?;
            modifier = Modifier::Spread;
        }

        let name = input.parse()?;

        if matches!(modifier, Modifier::None) && input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            modifier = Modifier::Optional;
        }

        input.parse::<Token![:]>()?;

        let ty = input.parse()?;

        let constant = if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        Ok(Self {
            attrs,
            modifier,
            name,
            ty,
            constant,
        })
    }
}

struct IdentReplacer {
    from: Ident,
    to: Ident,
}

impl VisitMut for IdentReplacer {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(type_path) = ty
            && let Some(first_segment) = type_path.path.segments.first_mut()
            && first_segment.ident == self.from
        {
            first_segment.ident = self.to.clone();
        }

        // Continue recursively visiting all types
        syn::visit_mut::visit_type_mut(self, ty);
    }
}

fn replace_ident_in_type(ty: &mut Type, from: Ident, to: Ident) {
    let mut replacer = IdentReplacer { from, to };
    replacer.visit_type_mut(ty);
}

pub(crate) fn impl_conditions(input: TokenStream) -> TokenStream {
    let Conditions {
        attrs: enum_attrs,
        name,
        generic: enum_generic,
        items,
    } = parse_macro_input!(input as Conditions);

    let mut variants = Vec::new();
    let mut conditions = Vec::new();
    let mut impls = Vec::new();
    let mut condition_impls = Vec::new();
    let mut condition_list_impls = Vec::new();

    for Condition {
        attrs: condition_attrs,
        name: condition,
        generics: generic_list,
        additional_derives,
        fields,
    } in items
    {
        let generics_original = generic_list.as_ref().map(|idents| quote!( < #idents > ));
        let mut generics_remapped = Punctuated::new();
        for pair in generic_list.clone().unwrap_or_default().into_pairs() {
            match pair {
                Pair::Punctuated(_ident, comma) => {
                    generics_remapped.push_value(enum_generic.clone());
                    generics_remapped.push_punct(comma);
                }
                Pair::End(_ident) => {
                    generics_remapped.push_value(enum_generic.clone());
                }
            }
        }
        let generics_remapped = if generics_original.is_some() {
            Some(quote!( < #generics_remapped > ))
        } else {
            None
        };

        let variant_doc = format!("A [`{condition}`](conditions::{condition}) condition.");

        variants.push(quote! {
            #[doc = #variant_doc]
            #condition(conditions::#condition #generics_remapped),
        });

        let additional_derives = additional_derives.map(|AdditionalDerives { derives }| {
            let derives = derives.into_iter();
            quote! {
                #( #[derive( #derives )] )*
            }
        });

        let definitions = fields.clone().into_iter().map(|field| {
            let ConditionField {
                attrs,
                modifier,
                name,
                ty,
                constant,
            } = field;

            match modifier {
                Modifier::None if constant.is_some() => quote! {
                    #( #attrs )*
                    #[clvm(constant = #constant)]
                    pub #name: #ty,
                },
                Modifier::None => quote! {
                    #( #attrs )*
                    pub #name: #ty,
                },
                Modifier::Spread => quote! {
                    #( #attrs )*
                    #[clvm(rest)]
                    pub #name: #ty,
                },
                Modifier::Optional => quote! {
                    #( #attrs )*
                    #[clvm(default)]
                    pub #name: #ty,
                },
            }
        });

        let mut parameters_original = Vec::new();
        let mut parameters_remapped = Vec::new();

        fields.clone().into_iter().for_each(|field| {
            let ConditionField {
                name,
                mut ty,
                constant,
                ..
            } = field;

            if constant.is_some() {
                return;
            }

            parameters_original.push(quote! { #name: #ty });

            for generic in generic_list.clone().unwrap_or_default() {
                replace_ident_in_type(&mut ty, generic, enum_generic.clone());
            }

            parameters_remapped.push(quote! { #name: #ty });
        });

        let names = fields.into_iter().filter_map(|field| {
            let ConditionField { name, constant, .. } = field;
            if constant.is_some() {
                return None;
            }
            Some(quote! { #name })
        });

        let new_parameters = parameters_original.clone();
        let new_names = names.clone();

        let new_doc = format!("Creates a new [`{condition}`] condition from its arguments.");

        conditions.push(quote! {
            #( #condition_attrs )*
            #[derive(::clvm_traits::ToClvm, ::clvm_traits::FromClvm)]
            #[::clvm_traits::apply_constants]
            #[derive(Debug, Clone, PartialEq, Eq)]
            #additional_derives
            #[clvm(list)]
            pub struct #condition #generics_original {
                #( #definitions )*
            }

            impl #generics_original #condition #generics_original {
                #[doc = #new_doc]
                pub fn new( #( #new_parameters, )* ) -> Self {
                    Self { #( #new_names, )* }
                }
            }
        });

        let snake_case = Ident::new(
            &condition.to_string().to_case(Case::Snake),
            Span::call_site().into(),
        );

        let into_name = Ident::new(&format!("into_{snake_case}"), Span::call_site().into());
        let as_name = Ident::new(&format!("as_{snake_case}"), Span::call_site().into());
        let is_name = Ident::new(&format!("is_{snake_case}"), Span::call_site().into());

        let condition_parameters = parameters_remapped.clone();
        let condition_names = names.clone();

        let generics_remapped_turbofish = generics_remapped
            .clone()
            .map(|generics| quote!( ::#generics ));

        let link = format!("[`{condition}`](conditions::{condition})");
        let variant_link = format!("[`Condition::{condition}`]");

        let constructor_doc = format!("Creates a {variant_link} from the arguments of a {link}.");
        let into_doc =
            format!("Returns the inner {link} if this is a {variant_link}, or [`None`] otherwise.");
        let as_doc = format!(
            "Returns a reference to the inner {link} if this is a {variant_link}, or [`None`] otherwise."
        );
        let is_doc = format!("Returns `true` if this is a {variant_link}.");
        let builder_doc =
            format!("Adds a {link} condition to the end of the list and returns the updated list.");

        condition_impls.push(quote! {
            #[doc = #constructor_doc]
            pub fn #snake_case( #( #condition_parameters, )* ) -> Self {
                Self::#condition( conditions::#condition #generics_remapped_turbofish { #( #condition_names, )* } )
            }
        });

        condition_impls.push(quote! {
            #[doc = #into_doc]
            pub fn #into_name(self) -> Option<conditions::#condition #generics_remapped> {
                if let Self::#condition(inner) = self {
                    Some(inner)
                } else {
                    None
                }
            }
        });

        condition_impls.push(quote! {
            #[doc = #as_doc]
            pub fn #as_name(&self) -> Option<&conditions::#condition #generics_remapped> {
                if let Self::#condition(inner) = self {
                    Some(inner)
                } else {
                    None
                }
            }
        });

        condition_impls.push(quote! {
            #[doc = #is_doc]
            pub fn #is_name(&self) -> bool {
                matches!(self, Self::#condition(..))
            }
        });

        let condition_names = names.clone();

        condition_list_impls.push(quote! {
            #[doc = #builder_doc]
            pub fn #snake_case(self, #( #parameters_remapped, )* ) -> Self {
                self.with( conditions::#condition #generics_remapped_turbofish { #( #condition_names, )* } )
            }
        });

        impls.push(quote! {
            impl<#enum_generic> From<conditions::#condition #generics_remapped> for Condition<#enum_generic> {
                fn from(inner: conditions::#condition #generics_remapped) -> Self {
                    Self::#condition(inner)
                }
            }
        });
    }

    quote! {
        #( #enum_attrs )*
        #[non_exhaustive]
        #[derive(Debug, Clone, PartialEq, Eq, ::clvm_traits::ToClvm, ::clvm_traits::FromClvm)]
        #[clvm(transparent)]
        pub enum #name<#enum_generic = ::clvmr::NodePtr> {
            #( #variants )*
            /// Any condition that doesn't match one of the other variants, kept as its raw value.
            ///
            /// When parsing, each variant is tried in order and this is the fallback, so it holds
            /// conditions with unknown opcodes as well as known opcodes whose arguments don't fit
            /// the expected shape.
            Other(T),
        }

        impl<#enum_generic> #name<#enum_generic> {
            #( #condition_impls )*
        }

        impl<#enum_generic> crate::Conditions<#enum_generic> {
            #( #condition_list_impls )*
        }

        /// Strongly typed structs for each individual condition, along with related helper types.
        ///
        /// Each struct here is the payload of one [`Condition`] variant and can be serialized to or
        /// parsed from CLVM on its own. This module also re-exports everything from the parent
        /// module, as well as [`Memos`](chia_puzzle_types::Memos).
        pub mod conditions {
            pub use super::*;

            #( #conditions )*

            pub use nfts::*;
            pub use agg_sig::*;
            pub use chia_puzzle_types::Memos;
        }

        #( #impls )*
    }
    .into()
}
