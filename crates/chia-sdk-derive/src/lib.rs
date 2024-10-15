use proc_macro::TokenStream;

mod impl_conditions;

use impl_conditions::impl_conditions;

#[proc_macro]
pub fn conditions(input: TokenStream) -> TokenStream {
    impl_conditions(input)
}
