use convert_case::{Case, Casing};
use proc_macro::{Span, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Expr, ExprLit, ExprTuple, ExprUnary, GenericArgument, Ident, Lit, PathArguments,
    Token, Type, UnOp, braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::{Pair, Punctuated},
    visit_mut::VisitMut,
};

#[derive(Debug, Clone)]
struct Conditions {
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
    modifier: Modifier,
    name: Ident,
    ty: Type,
    constant: Option<Expr>,
    /// Whether this is the trailing `rest` field that was added automatically.
    /// It isn't a constructor parameter, and defaults to nil.
    implicit: bool,
}

/// The canonical opcode atom, and each variant (with its type) that shares it, in declaration order.
type OpcodeGroup = (Vec<u8>, Vec<(Ident, TokenStream2)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    /// A single list element.
    None,
    /// The rest of the list, from this point on.
    Spread,
    /// A single list element, which may be absent if the list ends here.
    Optional,
    /// As many list elements as there are, until the list ends.
    Items,
}

impl Parse for Conditions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
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
        let mut modifier = Modifier::None;

        if input.peek(Token![...]) {
            input.parse::<Token![...]>()?;
            modifier = Modifier::Spread;
        } else if input.peek(Token![..]) {
            input.parse::<Token![..]>()?;
            modifier = Modifier::Items;
        }

        let name = input.parse()?;

        if modifier == Modifier::None && input.peek(Token![?]) {
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
            modifier,
            name,
            ty,
            constant,
            implicit: false,
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
        name,
        generic: enum_generic,
        items,
        ..
    } = parse_macro_input!(input as Conditions);

    match impl_conditions_inner(&name, &enum_generic, items) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn impl_conditions_inner(
    name: &Ident,
    enum_generic: &Ident,
    items: Punctuated<Condition, Token![,]>,
) -> syn::Result<TokenStream2> {
    let mut variants = Vec::new();
    let mut conditions = Vec::new();
    let mut impls = Vec::new();
    let mut condition_impls = Vec::new();
    let mut condition_list_impls = Vec::new();
    let mut opcode_groups: Vec<OpcodeGroup> = Vec::new();

    for Condition {
        attrs,
        name: condition,
        generics: generic_list,
        additional_derives,
        mut fields,
    } in items
    {
        let mut generic_list = generic_list.unwrap_or_default();
        let rest_generic = Ident::new("R", Span::call_site().into());

        // Every condition keeps any arguments that follow the ones it knows about, so that it can be
        // serialized back to exactly what it was parsed from. If the last field already consumes the
        // rest of the list, there's nothing left to keep.
        //
        // This has its own generic parameter, since the other parameters (such as the memos of a
        // `CREATE_COIN`) aren't necessarily able to represent nil. Use `()` for no trailing arguments,
        // which can be encoded with any encoder.
        if !fields
            .last()
            .is_some_and(|field| field.modifier == Modifier::Spread)
        {
            if generic_list.iter().any(|ident| *ident == rest_generic) {
                return Err(syn::Error::new(
                    condition.span(),
                    "the `R` generic parameter is reserved for the trailing arguments",
                ));
            }

            generic_list.push(rest_generic.clone());

            fields.push(ConditionField {
                modifier: Modifier::Spread,
                name: Ident::new("rest", Span::call_site().into()),
                ty: syn::parse_quote!(#rest_generic),
                constant: None,
                implicit: true,
            });
        }

        let has_implicit_rest = fields.iter().any(|field| field.implicit);
        let generic_idents: Vec<Ident> = generic_list.iter().cloned().collect();
        let has_generics = !generic_idents.is_empty();

        let generics_original = has_generics.then(|| quote!( < #( #generic_idents ),* > ));
        let generics_with_defaults =
            has_generics.then(|| quote!( < #( #generic_idents = ::clvmr::NodePtr ),* > ));

        let mut generics_remapped = Punctuated::<Ident, Token![,]>::new();
        for pair in generic_list.clone().into_pairs() {
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
        let generics_remapped = has_generics.then(|| quote!( < #generics_remapped > ));

        variants.push(quote! {
            #( #attrs )*
            #condition(conditions::#condition #generics_remapped),
        });

        let opcode = condition_opcode(&fields)?;

        let variant_ty = quote!( conditions::#condition #generics_remapped );

        if let Some((_, group)) = opcode_groups.iter_mut().find(|(bytes, _)| *bytes == opcode) {
            group.push((condition.clone(), variant_ty));
        } else {
            opcode_groups.push((opcode, vec![(condition.clone(), variant_ty)]));
        }

        let additional_derives = additional_derives.map(|AdditionalDerives { derives }| {
            let derives = derives.into_iter();
            quote! {
                #( #[derive( #derives )] )*
            }
        });

        let definitions = fields.iter().filter(|field| field.constant.is_none()).map(
            |ConditionField { name, ty, .. }| {
                quote! {
                    pub #name: #ty,
                }
            },
        );

        let (from_clvm_body, from_clvm_bounds) = from_clvm_body(&fields)?;
        let (to_clvm_body, to_clvm_bounds) = to_clvm_body(&fields)?;

        let mut parameters_original = Vec::new();
        let mut parameters_remapped = Vec::new();
        let mut names = Vec::new();
        let mut implicit_names = Vec::new();

        for field in &fields {
            if field.constant.is_some() {
                continue;
            }

            let name = &field.name;

            if field.implicit {
                implicit_names.push(quote! { #name });
                continue;
            }

            let ty = &field.ty;
            parameters_original.push(quote! { #name: #ty });

            let mut ty = ty.clone();
            for generic in &generic_idents {
                replace_ident_in_type(&mut ty, generic.clone(), enum_generic.clone());
            }

            parameters_remapped.push(quote! { #name: #ty });
            names.push(quote! { #name });
        }

        let default_bound = has_implicit_rest.then(|| quote!(where #rest_generic: crate::Nil));
        let enum_default_bound = has_implicit_rest.then(|| quote!(where #enum_generic: crate::Nil));

        conditions.push(quote! {
            #[derive(Debug, Clone, PartialEq, Eq)]
            #additional_derives
            #( #attrs )*
            pub struct #condition #generics_with_defaults {
                #( #definitions )*
            }

            impl #generics_original #condition #generics_original {
                pub fn new( #( #parameters_original, )* ) -> Self #default_bound {
                    Self {
                        #( #names, )*
                        #( #implicit_names: crate::Nil::nil(), )*
                    }
                }
            }

            impl<D, #( #generic_idents ),*> ::clvm_traits::FromClvm<D> for #condition #generics_original
            where
                D: ::clvm_traits::ClvmDecoder,
                #( #from_clvm_bounds, )*
            {
                #[allow(unused_mut)]
                fn from_clvm(
                    decoder: &D,
                    mut node: D::Node,
                ) -> ::std::result::Result<Self, ::clvm_traits::FromClvmError> {
                    #from_clvm_body
                }
            }

            impl<E, #( #generic_idents ),*> ::clvm_traits::ToClvm<E> for #condition #generics_original
            where
                E: ::clvm_traits::ClvmEncoder,
                #( #to_clvm_bounds, )*
            {
                fn to_clvm(
                    &self,
                    encoder: &mut E,
                ) -> ::std::result::Result<E::Node, ::clvm_traits::ToClvmError> {
                    #to_clvm_body
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

        let generics_remapped_turbofish = generics_remapped
            .clone()
            .map(|generics| quote!( ::#generics ));

        condition_impls.push(quote! {
            pub fn #snake_case( #( #parameters_remapped, )* ) -> Self #enum_default_bound {
                Self::#condition( conditions::#condition #generics_remapped_turbofish {
                    #( #names, )*
                    #( #implicit_names: crate::Nil::nil(), )*
                } )
            }
        });

        condition_impls.push(quote! {
            pub fn #into_name(self) -> Option<conditions::#condition #generics_remapped> {
                if let Self::#condition(inner) = self {
                    Some(inner)
                } else {
                    None
                }
            }
        });

        condition_impls.push(quote! {
            pub fn #as_name(&self) -> Option<&conditions::#condition #generics_remapped> {
                if let Self::#condition(inner) = self {
                    Some(inner)
                } else {
                    None
                }
            }
        });

        condition_impls.push(quote! {
            pub fn #is_name(&self) -> bool {
                matches!(self, Self::#condition(..))
            }
        });

        condition_list_impls.push(quote! {
            pub fn #snake_case(self, #( #parameters_remapped, )* ) -> Self #enum_default_bound {
                self.with( conditions::#condition #generics_remapped_turbofish {
                    #( #names, )*
                    #( #implicit_names: crate::Nil::nil(), )*
                } )
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

    let variant_bounds = opcode_groups
        .iter()
        .flat_map(|(_, group)| group.iter().map(|(_, ty)| ty));

    let opcode_arms = opcode_groups.iter().map(|(opcode, group)| {
        let parsers = group.iter().map(|(variant, ty)| {
            quote! {
                <#ty as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, node.clone()).map(Self::#variant)
            }
        });

        quote! {
            [ #( #opcode ),* ] => {
                let mut parsed = None;
                let mut error = None;

                #(
                    match #parsers {
                        Ok(condition) => {
                            if parsed.is_some() {
                                return Err(::clvm_traits::FromClvmError::Custom(
                                    "condition matches more than one variant with the same opcode".to_string(),
                                ));
                            }

                            parsed = Some(condition);
                        }
                        Err(err) => {
                            error.get_or_insert(err);
                        }
                    }
                )*

                match parsed {
                    Some(condition) => Ok(condition),
                    None => Err(error.expect("opcode group is never empty")),
                }
            }
        }
    });

    Ok(quote! {
        /// A condition output by a puzzle.
        ///
        /// Parsing follows the consensus rules for every condition that consensus knows about. A condition
        /// that consensus would reject is an error, a timelock that consensus treats as always true is
        /// [`Condition::TriviallyTrue`], and anything else must parse as exactly one of the variants that
        /// share its opcode. Every parsed condition serializes back to exactly the same value.
        ///
        /// Equality is structural for typed fields, but `NodePtr` values are compared by reference, so
        /// two separately allocated copies of the same value aren't equal. Compare tree hashes instead
        /// if you need structural equality.
        #[non_exhaustive]
        #[derive(Debug, Clone, PartialEq, Eq, ::clvm_traits::ToClvm)]
        #[clvm(transparent)]
        pub enum #name<#enum_generic = ::clvmr::NodePtr> {
            #( #variants )*
            /// A timelock condition with a value that consensus treats as always true, such as a negative
            /// number of seconds. It's kept as is, so that it can be serialized back to the same value.
            ///
            /// A trivially true relative timelock still prevents the spend from being ephemeral.
            TriviallyTrue(#enum_generic),
            /// A condition with an opcode that isn't known to consensus or any of the other variants.
            Other(#enum_generic),
        }

        impl<D, #enum_generic> ::clvm_traits::FromClvm<D> for #name<#enum_generic>
        where
            D: ::clvm_traits::ClvmDecoder,
            #enum_generic: ::clvm_traits::FromClvm<D>,
            #( #variant_bounds: ::clvm_traits::FromClvm<D>, )*
        {
            fn from_clvm(
                decoder: &D,
                node: D::Node,
            ) -> ::std::result::Result<Self, ::clvm_traits::FromClvmError> {
                // Consensus requires every condition to be a pair. If the opcode isn't an atom, it's ignored.
                let (opcode, args) = decoder.decode_pair(&node)?;

                let Ok(opcode) = decoder.decode_atom(&opcode) else {
                    return <#enum_generic as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, node).map(Self::Other);
                };

                if crate::check_consensus(decoder, opcode.as_ref(), args)? == crate::ConsensusCheck::TriviallyTrue {
                    return <#enum_generic as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, node).map(Self::TriviallyTrue);
                }

                // The opcode is matched against its exact atom bytes, the same way consensus and the CLVM `=`
                // operator do. A condition with a known opcode must parse as exactly one of the variants that
                // share that opcode, and is an error if it matches none or more than one of them.
                // This means `Other` can only contain conditions with an opcode that isn't otherwise known.
                match opcode.as_ref() {
                    #( #opcode_arms )*
                    _ => <#enum_generic as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, node).map(Self::Other),
                }
            }
        }

        impl<#enum_generic> #name<#enum_generic> {
            #( #condition_impls )*
        }

        impl<#enum_generic> crate::Conditions<#enum_generic> {
            #( #condition_list_impls )*
        }

        pub mod conditions {
            pub use super::*;

            #( #conditions )*

            pub use nfts::*;
            pub use agg_sig::*;
            pub use chia_puzzle_types::Memos;
        }

        #( #impls )*
    })
}

/// Generates the body of `from_clvm` for a condition, along with the trait bounds it needs.
fn from_clvm_body(
    fields: &Punctuated<ConditionField, Token![,]>,
) -> syn::Result<(TokenStream2, Vec<TokenStream2>)> {
    let mut body = TokenStream2::new();
    let mut bounds = Vec::new();
    let mut names = Vec::new();

    for (i, field) in fields.iter().enumerate() {
        let name = &field.name;
        let ty = &field.ty;
        let is_last = i == fields.len() - 1;

        if let Some(constant) = &field.constant {
            let bytes = constant_bytes(constant)?;

            match field.modifier {
                Modifier::None => body.extend(quote! {
                    let (first, rest) = decoder.decode_pair(&node)?;
                    crate::expect_atom(decoder, &first, &[ #( #bytes ),* ])?;
                    node = rest;
                }),
                Modifier::Spread => body.extend(quote! {
                    crate::expect_atom(decoder, &node, &[ #( #bytes ),* ])?;
                }),
                Modifier::Optional | Modifier::Items => {
                    return Err(syn::Error::new(
                        name.span(),
                        "constant fields can't be optional or repeated",
                    ));
                }
            }

            continue;
        }

        names.push(quote!(#name));

        match field.modifier {
            Modifier::None => {
                let decode = decode_value(ty, &quote!(first));
                bounds.push(quote!(#ty: ::clvm_traits::FromClvm<D>));
                body.extend(quote! {
                    let (first, rest) = decoder.decode_pair(&node)?;
                    let #name = #decode;
                    node = rest;
                });
            }
            Modifier::Optional => {
                bounds.push(
                    quote!(<#ty as crate::OptionalElement>::Item: ::clvm_traits::FromClvm<D>),
                );
                body.extend(quote! {
                    let #name = if let Ok((first, rest)) = decoder.decode_pair(&node) {
                        node = rest;
                        <#ty as crate::OptionalElement>::from_item(Some(
                            <<#ty as crate::OptionalElement>::Item as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, first)?
                        ))
                    } else {
                        <#ty as crate::OptionalElement>::from_item(None)
                    };
                });
            }
            Modifier::Items => {
                if is_last {
                    return Err(syn::Error::new(
                        name.span(),
                        "repeated fields must be followed by a spread field",
                    ));
                }

                let item_ty = vec_item_type(ty)?;
                let decode = decode_value(&item_ty, &quote!(first));
                bounds.push(quote!(#item_ty: ::clvm_traits::FromClvm<D>));
                body.extend(quote! {
                    let mut #name = Vec::new();
                    while let Ok((first, rest)) = decoder.decode_pair(&node) {
                        #name.push(#decode);
                        node = rest;
                    }
                });
            }
            Modifier::Spread => {
                if !is_last {
                    return Err(syn::Error::new(
                        name.span(),
                        "spread fields must be the last field",
                    ));
                }

                let decode = decode_value(ty, &quote!(node));
                bounds.push(quote!(#ty: ::clvm_traits::FromClvm<D>));
                body.extend(quote! {
                    let #name = #decode;
                });
            }
        }
    }

    body.extend(quote! {
        Ok(Self { #( #names, )* })
    });

    Ok((body, bounds))
}

/// Generates the body of `to_clvm` for a condition, along with the trait bounds it needs.
/// The list is built from the end, so that each field can be prepended to the rest of it.
fn to_clvm_body(
    fields: &Punctuated<ConditionField, Token![,]>,
) -> syn::Result<(TokenStream2, Vec<TokenStream2>)> {
    let mut bounds = Vec::new();
    let mut body = TokenStream2::new();

    let (spread, elements): (Vec<&ConditionField>, Vec<&ConditionField>) = fields
        .iter()
        .partition(|field| field.modifier == Modifier::Spread);

    match spread.first() {
        Some(ConditionField {
            constant: Some(constant),
            ..
        }) => {
            let bytes = constant_bytes(constant)?;
            body.extend(quote! {
                let mut node = encoder.encode_atom(::clvm_traits::Atom::Borrowed(&[ #( #bytes ),* ]))?;
            });
        }
        Some(ConditionField { name, ty, .. }) => {
            bounds.push(quote!(#ty: ::clvm_traits::ToClvm<E>));
            body.extend(quote! {
                let mut node = self.#name.to_clvm(encoder)?;
            });
        }
        None => body.extend(quote! {
            let mut node = encoder.encode_atom(::clvm_traits::Atom::Borrowed(&[]))?;
        }),
    }

    for field in elements.into_iter().rev() {
        let name = &field.name;
        let ty = &field.ty;

        if let Some(constant) = &field.constant {
            let bytes = constant_bytes(constant)?;
            body.extend(quote! {
                let first = encoder.encode_atom(::clvm_traits::Atom::Borrowed(&[ #( #bytes ),* ]))?;
                node = encoder.encode_pair(first, node)?;
            });
            continue;
        }

        match field.modifier {
            Modifier::None => {
                bounds.push(quote!(#ty: ::clvm_traits::ToClvm<E>));
                body.extend(quote! {
                    let first = self.#name.to_clvm(encoder)?;
                    node = encoder.encode_pair(first, node)?;
                });
            }
            Modifier::Optional => {
                bounds
                    .push(quote!(<#ty as crate::OptionalElement>::Item: ::clvm_traits::ToClvm<E>));
                body.extend(quote! {
                    if let Some(item) = crate::OptionalElement::item(&self.#name) {
                        let first = item.to_clvm(encoder)?;
                        node = encoder.encode_pair(first, node)?;
                    }
                });
            }
            Modifier::Items => {
                let item_ty = vec_item_type(ty)?;
                bounds.push(quote!(#item_ty: ::clvm_traits::ToClvm<E>));
                body.extend(quote! {
                    for item in self.#name.iter().rev() {
                        let first = item.to_clvm(encoder)?;
                        node = encoder.encode_pair(first, node)?;
                    }
                });
            }
            Modifier::Spread => unreachable!(),
        }
    }

    body.extend(quote! {
        Ok(node)
    });

    Ok((body, bounds))
}

/// Integers must be canonically encoded, so that they serialize back to the same atom.
fn decode_value(ty: &Type, node: &TokenStream2) -> TokenStream2 {
    if is_integer(ty) {
        quote!(crate::decode_canonical_int::<D, #ty>(decoder, #node)?)
    } else {
        quote!(<#ty as ::clvm_traits::FromClvm<D>>::from_clvm(decoder, #node)?)
    }
}

fn is_integer(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };

    path.qself.is_none()
        && path.path.get_ident().is_some_and(|ident| {
            matches!(
                ident.to_string().as_str(),
                "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128"
            )
        })
}

/// Extracts `T` from `Vec<T>`.
fn vec_item_type(ty: &Type) -> syn::Result<Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Vec"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(item)) = args.args.first()
    {
        return Ok(item.clone());
    }

    Err(syn::Error::new_spanned(
        ty,
        "repeated fields must be a `Vec`",
    ))
}

/// Finds the constant value of the `opcode` field, and encodes it as the canonical CLVM atom.
fn condition_opcode(fields: &Punctuated<ConditionField, Token![,]>) -> syn::Result<Vec<u8>> {
    let Some(field) = fields.iter().find(|field| field.name == "opcode") else {
        return Err(syn::Error::new(
            Span::call_site().into(),
            "conditions must have an `opcode` field",
        ));
    };

    let Some(constant) = &field.constant else {
        return Err(syn::Error::new(
            field.name.span(),
            "the `opcode` field must have a constant value",
        ));
    };

    constant_bytes(constant)
}

/// Encodes a constant as the canonical CLVM atom. Only integers and nil are supported.
fn constant_bytes(constant: &Expr) -> syn::Result<Vec<u8>> {
    let value = match constant {
        Expr::Tuple(ExprTuple { elems, .. }) if elems.is_empty() => return Ok(Vec::new()),
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse::<i64>()?,
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => match expr.as_ref() {
            Expr::Lit(ExprLit {
                lit: Lit::Int(int), ..
            }) => -int.base10_parse::<i64>()?,
            _ => return Err(syn::Error::new_spanned(constant, "expected an integer")),
        },
        _ => {
            return Err(syn::Error::new_spanned(
                constant,
                "expected an integer or `()`",
            ));
        }
    };

    Ok(encode_int(value))
}

/// Encodes an integer as a minimal signed big-endian CLVM atom.
fn encode_int(value: i64) -> Vec<u8> {
    if value == 0 {
        return Vec::new();
    }

    let mut bytes = value.to_be_bytes().to_vec();

    while bytes.len() > 1
        && ((bytes[0] == 0x00 && bytes[1] & 0x80 == 0)
            || (bytes[0] == 0xff && bytes[1] & 0x80 != 0))
    {
        bytes.remove(0);
    }

    bytes
}
