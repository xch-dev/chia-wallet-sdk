//! Procedural macros used by [`chia-sdk-types`](https://docs.rs/chia-sdk-types).
//!
//! This crate is an implementation detail of `chia-sdk-types` and isn't meant to be used directly.

#![cfg_attr(not(test), warn(missing_docs))]

use proc_macro::TokenStream;

mod impl_conditions;

use impl_conditions::impl_conditions;

/// Generates the `Condition` enum, one struct per condition, and their helper methods.
///
/// This macro is used internally by `chia-sdk-types` and relies on items that only exist in that
/// crate (such as `crate::Conditions` and the `nfts` and `agg_sig` modules), so it can't be used
/// anywhere else.
///
/// The input looks like an enum declaration where each variant has a list of fields:
///
/// ```text
/// /// Docs for the enum.
/// pub enum Condition<T> {
///     /// Docs for the condition struct.
///     CreateCoin<T> as Copy {
///         opcode: i8 if 51,
///         /// Docs for the field.
///         puzzle_hash: Bytes32,
///         amount: u64,
///         ...memos: Memos<T>,
///     },
/// }
/// ```
///
/// For each entry, the macro generates:
///
/// - A `conditions::Name` struct that derives `ToClvm` and `FromClvm` as a CLVM list, plus
///   `Debug`, `Clone`, `PartialEq`, `Eq`, and any extra derives listed after `as` (joined by `+`).
/// - A `new` constructor that takes every field that isn't a constant.
/// - A `Condition::Name` variant, along with `Condition::name`, `into_name`, `as_name`, and
///   `is_name` methods, and a `From<Name>` impl for `Condition`.
/// - A builder method `Conditions::name` that appends the condition to a list.
///
/// Field syntax:
///
/// - `field: Type if value` is a constant (such as the opcode) that's checked when parsing and
///   isn't stored in the struct.
/// - `...field: Type` captures the rest of the list.
/// - `field?: Type` is optional and uses the type's default when missing.
///
/// Doc comments (and any other outer attributes) on the enum, on each entry, and on each field are
/// forwarded to the generated items.
#[proc_macro]
pub fn conditions(input: TokenStream) -> TokenStream {
    impl_conditions(input)
}
