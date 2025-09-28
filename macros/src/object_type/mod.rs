use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitInt, Result, parse::Parse};
mod keywords {
    use syn::custom_keyword;
    custom_keyword!(size);
    custom_keyword!(const_size);

    custom_keyword!(type_key);
}
struct ObjectTypeAttributes {
    size: Option<LitInt>,
    const_size: Option<LitInt>,
    type_key: Option<LitInt>,
}
impl Parse for ObjectTypeAttributes {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut size: Option<LitInt> = None;
        let mut const_size: Option<LitInt> = None;
        let mut type_key: Option<LitInt> = None;

        while !input.is_empty() {
            if input.peek(keywords::size) {
                input.parse::<keywords::size>()?;
                input.parse::<syn::Token![=]>()?;
                size = Some(input.parse()?);
            } else if input.peek(keywords::const_size) {
                input.parse::<keywords::const_size>()?;
                input.parse::<syn::Token![=]>()?;

                const_size = Some(input.parse()?);
            } else if input.peek(keywords::type_key) {
                input.parse::<keywords::type_key>()?;
                input.parse::<syn::Token![=]>()?;
                type_key = Some(input.parse()?);
            } else {
                return Err(input.error("Expected size, const_size, or type_key attribute"));
            }
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else {
                break;
            }
        }
        if let Some(const_size) = const_size.as_ref()
            && size.is_none()
        {
            size = Some(LitInt::new(const_size.base10_digits(), const_size.span()));
        }
        Ok(ObjectTypeAttributes {
            size,
            const_size,
            type_key,
        })
    }
}
pub fn expand(input: DeriveInput) -> Result<TokenStream> {
    let DeriveInput { ident, attrs, .. } = input;
    let parsed_attrs = attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("object_type") {
                Some(attr.parse_args::<ObjectTypeAttributes>())
            } else {
                None
            }
        })
        .transpose()?;
    let Some(attrs) = parsed_attrs else {
        return Err(syn::Error::new_spanned(
            ident.clone(),
            "object_type attribute is required",
        ));
    };
    let const_size = attrs
        .const_size
        .map(|lit| {
            quote! {
                fn const_size(&self) -> Option<usize> {
                    Some(#lit)
                }
            }
        })
        .unwrap_or_default();

    let object_type_impl = if let Some(size) = attrs.size {
        quote! {
            impl TuxIOType for #ident {
                fn size(&self) -> usize {
                    #size
                }
                #const_size
            }
        }
    } else {
        quote! {}
    };
    let type_key_impl = if let Some(type_key) = attrs.type_key {
        quote! {
            impl TypedObjectType for #ident {
                fn type_key() -> u8 {
                    #type_key
                }
            }
            impl ConstTypedObjectType for #ident {
                const TYPE_KEY: u8 = #type_key;
            }
        }
    } else {
        quote! {}
    };
    let result = quote! {
        #object_type_impl
        #type_key_impl
    };
    Ok(result)
}
