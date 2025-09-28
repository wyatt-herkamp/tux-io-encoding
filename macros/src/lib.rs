mod object_type;
mod value_enum;
#[proc_macro_derive(ObjectType, attributes(object_type))]
pub fn object_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let expanded = object_type::expand(input).unwrap_or_else(|err| err.to_compile_error());
    proc_macro::TokenStream::from(expanded)
}
#[proc_macro_derive(ValueEnum)]
pub fn value_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let expanded = value_enum::expand(input).unwrap_or_else(|err| err.to_compile_error());
    proc_macro::TokenStream::from(expanded)
}
