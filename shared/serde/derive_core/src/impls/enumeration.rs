use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{DataEnum, Fields, Generics};

use super::structure::{reject_lifetimes, reject_unsupported_field_type, wire_schema_generics};

/// Number of bits needed to encode any of `variant_count` distinct
/// indices `0..variant_count`. Equivalent to `ceil(log2(variant_count))`,
/// with a floor of 1 bit so that `UnsignedInteger<N>` (which rejects
/// `N == 0`) stays valid for trivial 1-variant enums.
fn bits_needed_for(variant_count: usize) -> u8 {
    if variant_count <= 2 {
        return 1;
    }
    let max_index = variant_count - 1;
    let bits = usize::BITS - max_index.leading_zeros();
    if bits >= 256 {
        panic!("cannot encode a number in more than 255 bits!");
    }
    bits as u8
}

/// Derives `WireSchema` for an enum: the real `bits_needed_for` width plus,
/// per variant in declaration order, its ordinal, label, and payload
/// descriptor. Named-variant payloads describe as labeled structs (labels
/// matter, exactly as in serialization order); tuple-variant payloads
/// describe as ordered tuples; unit variants carry no payload.
pub fn derive_wire_schema_enum(
    enum_: &DataEnum,
    enum_name: &Ident,
    generics: &Generics,
    schema_crate: &TokenStream,
) -> TokenStream {
    if let Some(rejection) = reject_lifetimes(generics) {
        return rejection;
    }
    let variant_number = enum_.variants.len();
    let bits_needed = bits_needed_for(variant_number);

    let mut variant_count = 0u32;
    let mut variant_tokens = quote! {};
    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_ordinal = index as u32;
        let variant_label = variant.ident.to_string();
        let payload = match &variant.fields {
            Fields::Unit => quote! {
                out.push(0u8);
            },
            Fields::Named(fields) => {
                let mut payload_count = 0u32;
                let mut payload_tokens = quote! {};
                for field in &fields.named {
                    let Some(field_name) = field.ident.as_ref() else {
                        continue;
                    };
                    if let Some(rejection) = reject_unsupported_field_type(&field.ty) {
                        return rejection;
                    }
                    let field_label = field_name.to_string();
                    let field_ty = &field.ty;
                    payload_count += 1;
                    payload_tokens = quote! {
                        #payload_tokens
                        #schema_crate::wire_schema_label(out, #field_label);
                        #schema_crate::wire_schema_field::<#field_ty>(ctx, out);
                    };
                }
                quote! {
                    out.push(1u8);
                    out.push(#schema_crate::SCHEMA_TAG_STRUCT);
                    #schema_crate::wire_schema_count(out, #payload_count);
                    #payload_tokens
                }
            }
            Fields::Unnamed(fields) => {
                let mut payload_count = 0u32;
                let mut payload_tokens = quote! {};
                for field in &fields.unnamed {
                    if let Some(rejection) = reject_unsupported_field_type(&field.ty) {
                        return rejection;
                    }
                    let field_ty = &field.ty;
                    payload_count += 1;
                    payload_tokens = quote! {
                        #payload_tokens
                        #schema_crate::wire_schema_field::<#field_ty>(ctx, out);
                    };
                }
                quote! {
                    out.push(1u8);
                    out.push(#schema_crate::SCHEMA_TAG_TUPLE);
                    #schema_crate::wire_schema_count(out, #payload_count);
                    #payload_tokens
                }
            }
        };
        variant_count += 1;
        variant_tokens = quote! {
            #variant_tokens
            #schema_crate::wire_schema_count(out, #variant_ordinal);
            #schema_crate::wire_schema_label(out, #variant_label);
            #payload
        };
    }

    let (impl_generics, ty_generics, where_clause) = wire_schema_generics(generics, schema_crate);

    // Flat emission with absolute paths (see the struct shape).
    quote! {
        impl #impl_generics #schema_crate::WireSchema for #enum_name #ty_generics #where_clause {
            fn wire_schema(
                ctx: &mut #schema_crate::WireSchemaContext,
                out: &mut Vec<u8>,
            ) {
                out.push(#schema_crate::SCHEMA_TAG_ENUM);
                out.push(#bits_needed);
                #schema_crate::wire_schema_count(out, #variant_count);
                #variant_tokens
            }
        }
    }
}

/// Shared entry: emits `impl Serde` (historical) plus `impl WireSchema` from
/// the same shape, so the two can never drift.
#[allow(clippy::format_push_string)]
pub fn derive_serde_enum(
    enum_: &DataEnum,
    enum_name: &Ident,
    generics: &Generics,
    serde_crate_name: TokenStream,
) -> TokenStream {
    let variant_number = enum_.variants.len();
    let bits_needed = bits_needed_for(variant_number);

    let ser_method = get_ser_method(enum_, bits_needed);
    let de_method = get_de_method(enum_, bits_needed);
    let bit_length_method = get_bit_length_method(enum_, bits_needed);

    let lowercase_enum_name = Ident::new(
        enum_name.to_string().to_lowercase().as_str(),
        Span::call_site(),
    );
    let module_name = format_ident!("define_serde_{}", lowercase_enum_name);

    let import_types =
        quote! { Serde, BitWrite, UnsignedInteger, BitReader, SerdeErr, ConstBitLength, };
    let imports = quote! { use #serde_crate_name::{#import_types}; };

    let schema_impl = derive_wire_schema_enum(enum_, enum_name, generics, &serde_crate_name);

    quote! {
        mod #module_name {
            #imports
            use super::#enum_name;

            impl Serde for #enum_name {
                #ser_method
                #de_method
                #bit_length_method
            }
        }
        #schema_impl
    }
}

fn get_ser_method(enum_: &DataEnum, bits_needed: u8) -> TokenStream {
    let mut ser = quote! {};
    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_index = index as u16;
        let variant_name = &variant.ident;
        let base = match &variant.fields {
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {
                        let index = UnsignedInteger::<#bits_needed>::new(#variant_index);
                        index.ser(writer);
                    }
                }
            }
            Fields::Named(fields) => {
                let names: Vec<&Ident> = fields
                    .named
                    .iter()
                    .map(|field| {
                        field
                            .ident
                            .as_ref()
                            .expect("expected field to have a name.")
                    })
                    .collect();
                let left = quote! { Self::#variant_name{ #(#names),* } };
                let mut right = quote! {
                    let index = UnsignedInteger::<#bits_needed>::new(#variant_index);
                    index.ser(writer);
                };
                for field in fields.named.iter() {
                    let field_name = field
                        .ident
                        .as_ref()
                        .expect("expected field to have a name.");
                    right = quote! {
                        #right
                        #field_name.ser(writer);
                    }
                }
                quote! {
                    #left => { #right }
                }
            }
            Fields::Unnamed(fields) => {
                let names: Vec<Ident> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format_ident!("f{}", i))
                    .collect();
                let left = quote! { Self::#variant_name( #(#names),* ) };

                let mut right = quote! {
                    let index = UnsignedInteger::<#bits_needed>::new(#variant_index);
                    index.ser(writer);
                };
                for field_name in names {
                    right = quote! {
                        #right
                        #field_name.ser(writer);
                    }
                }
                quote! {
                    #left => { #right }
                }
            }
        };
        ser = quote! {
            #ser
            #base
        }
    }
    quote! {
         fn ser(&self, writer: &mut dyn BitWrite) {
            match self {
                #ser
            }
         }
    }
}

fn get_de_method(enum_: &DataEnum, bits_needed: u8) -> TokenStream {
    let mut de = quote! {};

    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_index = index as u16;
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Unit => {
                de = quote! {
                    #de
                    #variant_index => Self::#variant_name,
                }
            }
            Fields::Named(fields) => {
                let mut base = quote! {};
                for field in fields.named.iter() {
                    let field_name = field
                        .ident
                        .as_ref()
                        .expect("expected field to have a name.");
                    base = quote! {
                        #base
                        #field_name: Serde::de(reader)?,
                    }
                }
                de = quote! {
                    #de
                    #variant_index => Self::#variant_name{
                        #base
                    },
                }
            }
            Fields::Unnamed(fields) => {
                let mut base = quote! {};
                for _ in fields.unnamed.iter() {
                    base = quote! {
                        #base
                        Serde::de(reader)?,
                    }
                }
                de = quote! {
                    #de
                    #variant_index => Self::#variant_name(
                        #base
                    ),
                }
            }
        }
    }
    quote! {
        fn de(reader: &mut BitReader) -> std::result::Result<Self, SerdeErr> {
            let index: UnsignedInteger<#bits_needed> = Serde::de(reader)?;
            let index_u16: u16 = index.get() as u16;
            Ok(match index_u16 {
                #de
                _ => return Err(SerdeErr)
            })
        }
    }
}

fn get_bit_length_method(enum_: &DataEnum, bits_needed: u8) -> TokenStream {
    let mut bit_length = quote! {};
    for variant in enum_.variants.iter() {
        let variant_name = &variant.ident;
        let base = match &variant.fields {
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {
                        output += <UnsignedInteger::<#bits_needed> as ConstBitLength>::const_bit_length();
                    }
                }
            }
            Fields::Named(fields) => {
                let names: Vec<&Ident> = fields
                    .named
                    .iter()
                    .map(|field| {
                        field
                            .ident
                            .as_ref()
                            .expect("expected field to have a name.")
                    })
                    .collect();
                let left = quote! { Self::#variant_name{ #(#names),* } };
                let mut right = quote! {
                    output += <UnsignedInteger::<#bits_needed> as ConstBitLength>::const_bit_length();
                };
                for field in fields.named.iter() {
                    let field_name = field
                        .ident
                        .as_ref()
                        .expect("expected field to have a name.");
                    right = quote! {
                        #right
                        output += #field_name.bit_length();
                    }
                }
                quote! {
                    #left => { #right }
                }
            }
            Fields::Unnamed(fields) => {
                let names: Vec<Ident> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format_ident!("f{}", i))
                    .collect();
                let left = quote! { Self::#variant_name( #(#names),* ) };

                let mut right = quote! {
                    output += <UnsignedInteger::<#bits_needed> as ConstBitLength>::const_bit_length();
                };
                for field_name in names {
                    right = quote! {
                        #right
                        output += #field_name.bit_length();
                    }
                }
                quote! {
                    #left => { #right }
                }
            }
        };
        bit_length = quote! {
            #bit_length
            #base
        }
    }
    quote! {
         fn bit_length(&self) -> u32 {
            let mut output = 0;
            match self {
                #bit_length
            }
            output
         }
    }
}
