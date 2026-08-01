//! Derive macros for `tux-io-encoding`.
//!
//! Not useful on their own — see the [`tux-io-encoding`](https://docs.rs/tux-io-encoding) crate, which
//! re-exports both and defines the traits they implement.

mod object_type;
mod value_enum;

/// Implements `TuxIOType`, `TypedObjectType` and `ConstTypedObjectType` for a fixed-size type.
///
/// Only the size and the type key are generated. `WritableObjectType` and `ReadableObjectType` are
/// still written by hand, because how a type lays its fields out is the part worth being explicit
/// about.
///
/// # Attributes
///
/// `#[object_type(...)]` is required, and takes any of:
///
/// | Attribute | Effect |
/// | --------- | ------ |
/// | `const_size = N` | `const_size()` returns `Some(N)`, and `size()` returns `N` unless `size` overrides it |
/// | `size = N` | `size()` returns `N` |
/// | `type_key = N` | `TypedObjectType::type_key()` and `ConstTypedObjectType::TYPE_KEY` return `N` |
///
/// ```ignore
/// #[derive(ObjectType)]
/// #[object_type(const_size = 4, type_key = 13)]
/// pub struct RawDate {
///     pub year: u16,
///     pub month: u8,
///     pub day: u8,
/// }
/// ```
///
/// # Two things to know
///
/// **`size = N` generates a constant.** The generated `size()` returns the literal `N` for every value,
/// so it is only correct for a type whose encoding is genuinely fixed-width. A variable-size type must
/// implement `TuxIOType` by hand — using this attribute on one produces a `size()` that lies, and the
/// file layer reserves space from `size()`.
///
/// **Omitting both `size` and `const_size` emits no `TuxIOType` impl at all**, silently. The derive
/// succeeds and the trait is simply missing, which surfaces later as a confusing unsatisfied bound at
/// the use site rather than an error here.
///
/// A type key must match the registry in the `tux-io-encoding` crate documentation. Keys are part of the
/// on-disk format: reusing one makes existing files decode as the wrong type.
#[proc_macro_derive(ObjectType, attributes(object_type))]
pub fn object_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let expanded = object_type::expand(input).unwrap_or_else(|err| err.to_compile_error());
    proc_macro::TokenStream::from(expanded)
}

/// Implements the encoding traits for a dynamically-typed value enum.
///
/// Applied to `ValueType` in `tux-io-encoding`, which is the value half of every metadata and tag map.
/// Generates:
///
/// - `TuxIOType`, `WritableObjectType` and `ReadableObjectType`, dispatching on a one-byte type key
/// - `From<Inner>` for each variant, so `"text".to_owned().into()` builds one
/// - `From<ValueType> for Option<Inner>`, for getting back out
///
/// Every variant must be a tuple variant with exactly one field, and that field's type must implement
/// `ConstTypedObjectType` — its `TYPE_KEY` is what the generated reader matches on, so two variants
/// whose inner types share a key make the second unreachable.
///
/// The generated `size()` includes the type-key byte, matching what the generated writer emits and
/// what `read_size` counts. That is the invariant documented on `TuxIOType`, and it was violated here:
/// `size()` used to return the inner value's size alone, one byte short of the truth.
///
/// ```ignore
/// #[derive(Debug, Clone, PartialEq, ValueEnum)]
/// pub enum ValueType {
///     String(String),
///     U32(u32),
///     // ...
/// }
/// ```
///
/// Takes no attributes. The name `ValueType` is currently hardcoded in the generated `From` impls, so
/// the derive only works for an enum of that name.
#[proc_macro_derive(ValueEnum)]
pub fn value_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let expanded = value_enum::expand(input).unwrap_or_else(|err| err.to_compile_error());
    proc_macro::TokenStream::from(expanded)
}
