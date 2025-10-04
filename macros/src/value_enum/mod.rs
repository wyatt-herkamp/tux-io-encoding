use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Fields, Result, Variant};
pub struct ValueVariant {
    pub variant: Variant,
}
impl TryFrom<Variant> for ValueVariant {
    type Error = syn::Error;

    fn try_from(variant: Variant) -> Result<Self> {
        if let Fields::Unnamed(_) = variant.fields
            && variant.fields.len() == 1
        {
            Ok(ValueVariant { variant })
        } else {
            Err(syn::Error::new_spanned(
                variant,
                "ValueEnum variants must have exactly one unnamed field",
            ))
        }
    }
}
impl ValueVariant {
    fn inner_type(&self) -> &syn::Type {
        if let Fields::Unnamed(fields) = &self.variant.fields
            && fields.unnamed.len() == 1
        {
            &fields.unnamed[0].ty
        } else {
            unimplemented!("ValueEnum variants must have exactly one unnamed field");
        }
    }
    pub fn const_size(&self) -> TokenStream {
        let ident = &self.variant.ident;
        quote! {
            Self::#ident(v) => v.const_size(),
        }
    }
    pub fn size(&self) -> TokenStream {
        let ident = &self.variant.ident;
        quote! {
            Self::#ident(v) => v.size(),
        }
    }
    #[allow(clippy::wrong_self_convention)]
    pub fn from_impl(&self) -> TokenStream {
        let inner_type = self.inner_type();
        let ident = &self.variant.ident;

        quote! {
            impl core::convert::From<#inner_type> for ValueType {
                fn from(value: #inner_type) -> Self {
                    ValueType::#ident(value)
                }
            }
        }
    }
    #[allow(clippy::wrong_self_convention)]
    pub fn into_option(&self) -> TokenStream {
        let inner_type = self.inner_type();
        let ident = &self.variant.ident;

        quote! {
            impl core::convert::From<ValueType> for core::option::Option<#inner_type> {
                fn from(value: ValueType) -> Self {
                    match value {
                        ValueType::#ident(v) => core::option::Option::Some(v),
                        _ => core::option::Option::None,
                    }
                }
            }
        }
    }
    pub fn read_from_reader(&self) -> TokenStream {
        let inner_type = self.inner_type();
        let ident = &self.variant.ident;

        quote! {
            <#inner_type as ConstTypedObjectType>::TYPE_KEY => {
                let value = <#inner_type as ReadableObjectType>::read_from_reader(reader)?;
                Ok(ValueType::#ident(value))
            }
        }
    }
    pub fn read_size(&self) -> TokenStream {
        let inner_type = self.inner_type();

        quote! {
            <#inner_type as ConstTypedObjectType>::TYPE_KEY => {
                let value = <#inner_type as ReadableObjectType>::read_size(reader)?;
                Ok(value + 1)
            }
        }
    }
    pub fn write_to_writer(&self) -> TokenStream {
        let inner_type = self.inner_type();
        let ident = &self.variant.ident;

        quote! {
            ValueType::#ident(v) => {
                writer.write_all(&[<#inner_type as ConstTypedObjectType>::TYPE_KEY])?;
                v.write_to_writer(writer)
            }
        }
    }
}
pub fn expand(input: DeriveInput) -> Result<TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    let variants = match data {
        syn::Data::Enum(data_enum) => data_enum
            .variants
            .into_iter()
            .map(ValueVariant::try_from)
            .collect::<Result<Vec<_>>>()?,
        _ => {
            return Err(syn::Error::new_spanned(
                ident,
                "ValueEnum can only be derived for enums",
            ));
        }
    };
    let const_size_variants = variants.iter().map(|v| v.const_size()).collect::<Vec<_>>();
    let size_variants = variants.iter().map(|v| v.size()).collect::<Vec<_>>();
    let read_from_reader_variants = variants
        .iter()
        .map(|v| v.read_from_reader())
        .collect::<Vec<_>>();
    let write_to_writer_variants = variants
        .iter()
        .map(|v| v.write_to_writer())
        .collect::<Vec<_>>();
    let read_size_variants = variants.iter().map(|v| v.read_size()).collect::<Vec<_>>();

    let from_impl = variants.iter().map(|v| v.from_impl()).collect::<Vec<_>>();
    let into_option_impl = variants.iter().map(|v| v.into_option()).collect::<Vec<_>>();
    let expanded = quote! {
        #(#from_impl)*
        #(#into_option_impl)*

        impl TuxIOType for ValueType {
            fn const_size(&self) -> Option<usize> {
                match self {
                    #(#const_size_variants)*
                }
            }
            fn size(&self) -> usize {
                match self {
                    #(#size_variants)*
                }
            }
        }
        impl ReadableObjectType for ValueType {
            fn read_size<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<usize, EncodingError> {
                let type_key = u8::read_from_reader(reader)?;
                match type_key {
                    #(#read_size_variants)*
                    _ => Err(EncodingError::UnknownTypeKey(type_key)),
                }
            }
            fn read_from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, EncodingError>
            where
                Self: Sized,
            {
                let type_key = u8::read_from_reader(reader)?;
                match type_key {
                    #(#read_from_reader_variants)*
                    _ => Err(EncodingError::UnknownTypeKey(type_key)),
                }
            }
        }
        impl WritableObjectType for ValueType {
            fn write_to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), EncodingError> {
                match self {
                    #(#write_to_writer_variants)*
                }
            }
        }
    };

    Ok(expanded)
}
