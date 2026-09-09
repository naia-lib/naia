use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{DataStruct, Generics, Index};

use super::structure::{reject_lifetimes, reject_unsupported_field_type, wire_schema_generics};

/// Derives `WireSchema` for a tuple struct: element count plus each element
/// descriptor in order. Same generics contract as the named-field shape.
pub fn derive_wire_schema_tuple_struct(
    struct_: &DataStruct,
    struct_name: &Ident,
    generics: &Generics,
    schema_crate: &TokenStream,
) -> TokenStream {
    if let Some(rejection) = reject_lifetimes(generics) {
        return rejection;
    }

    let mut elem_count = 0u32;
    let mut elem_tokens = quote! {};
    for field in struct_.fields.iter() {
        if let Some(rejection) = reject_unsupported_field_type(&field.ty) {
            return rejection;
        }
        let field_ty = &field.ty;
        elem_count += 1;
        elem_tokens = quote! {
            #elem_tokens
            #schema_crate::wire_schema_field::<#field_ty>(ctx, out);
        };
    }

    let (impl_generics, ty_generics, where_clause) = wire_schema_generics(generics, schema_crate);

    // Flat emission with absolute paths (see the struct shape).
    quote! {
        impl #impl_generics #schema_crate::WireSchema for #struct_name #ty_generics #where_clause {
            fn wire_schema(
                ctx: &mut #schema_crate::WireSchemaContext,
                out: &mut Vec<u8>,
            ) {
                out.push(#schema_crate::SCHEMA_TAG_TUPLE);
                #schema_crate::wire_schema_count(out, #elem_count);
                #elem_tokens
            }
        }
    }
}

/// Shared entry: emits `impl Serde` (historical) plus `impl WireSchema` from
/// the same shape, so the two can never drift.
#[allow(clippy::format_push_string)]
pub fn derive_serde_tuple_struct(
    struct_: &DataStruct,
    struct_name: &Ident,
    generics: &Generics,
    serde_crate_name: TokenStream,
) -> TokenStream {
    let mut ser_body = quote! {};
    let mut de_body = quote! {};
    let mut bit_length_body = quote! {};

    for (i, _) in struct_.fields.iter().enumerate() {
        let field_index = Index::from(i);
        ser_body = quote! {
            #ser_body
            self.#field_index.ser(writer);
        };
        de_body = quote! {
            #de_body
            #field_index: Serde::de(reader)?,
        };
        bit_length_body = quote! {
            #bit_length_body
            output += self.#field_index.bit_length();
        };
    }

    let lowercase_struct_name = Ident::new(
        struct_name.to_string().to_lowercase().as_str(),
        Span::call_site(),
    );
    let module_name = format_ident!("define_{}", lowercase_struct_name);

    let import_types = quote! {BitWrite, Serde, ConstBitLength, BitReader, SerdeErr};
    let imports = quote! { use #serde_crate_name::{#import_types}; };

    let schema_impl =
        derive_wire_schema_tuple_struct(struct_, struct_name, generics, &serde_crate_name);

    quote! {
        mod #module_name {
            #imports
            use super::#struct_name;
            impl Serde for #struct_name {
                 fn ser(&self, writer: &mut dyn BitWrite) {
                    #ser_body
                 }
                 fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
                    Ok(Self {
                        #de_body
                    })
                 }
                 fn bit_length(&self) -> u32 {
                    let mut output = 0;
                    #bit_length_body
                    output
                }
            }
        }
        #schema_impl
    }
}
