use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{DataStruct, Fields, Generics, Type, WherePredicate};

// ── Shared WireSchema derive helpers (used by all three shapes) ──────────

/// Splits `generics` for the `WireSchema` impl, preserving the original
/// `where` clause and adding a `WireSchema` bound to every ordinary type
/// parameter that does not already carry one.
///
/// Only the parameters are bounded — never the implementing type itself —
/// so a recursive field (`Box<Self>`) cannot form a self-field where-bound
/// cycle. Const parameters need no bound: their values flow through the
/// field types that mention them.
pub(crate) fn wire_schema_generics(
    generics: &Generics,
    schema_crate: &TokenStream,
) -> (TokenStream, TokenStream, TokenStream) {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut where_clause: syn::WhereClause = where_clause
        .cloned()
        .unwrap_or_else(|| syn::parse_quote!(where));
    for param in &generics.params {
        let syn::GenericParam::Type(type_param) = param else {
            continue;
        };
        let ident = &type_param.ident;
        if bounds_wire_schema(&where_clause, ident) {
            continue;
        }
        where_clause
            .predicates
            .push(syn::parse_quote!(#ident: #schema_crate::WireSchema));
    }
    (
        quote! { #impl_generics },
        quote! { #ty_generics },
        quote! { #where_clause },
    )
}

/// Whether `where_clause` already bounds `ident` with `WireSchema`.
fn bounds_wire_schema(where_clause: &syn::WhereClause, ident: &Ident) -> bool {
    where_clause.predicates.iter().any(|predicate| {
        let WherePredicate::Type(bounded) = predicate else {
            return false;
        };
        let Type::Path(bounded_path) = &bounded.bounded_ty else {
            return false;
        };
        if !bounded_path.path.is_ident(ident) {
            return false;
        }
        bounded.bounds.iter().any(|bound| {
            matches!(bound, syn::TypeParamBound::Trait(bound_trait)
            if bound_trait.path.segments.last().is_some_and(
                |segment| segment.ident == "WireSchema"
            ))
        })
    })
}

/// Rejects field types no descriptor can honestly describe: references
/// (borrowed wire fields, including `&[T]`, whose deserialization rejects
/// them) and `GlobalEntity` (intentionally unsupported; its manual
/// serialization panics). Returns the `compile_error!` tokens, or `None`
/// when the type is describable. Recurses through generic arguments,
/// tuples, arrays, and slices so `Option<&str>` is caught too.
pub(crate) fn reject_unsupported_field_type(ty: &Type) -> Option<TokenStream> {
    fn reason(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Reference(_) => Some(
                "WireSchema derive rejects borrowed wire fields: \
                 references cannot be described (deserialization rejects them)",
            ),
            Type::Path(type_path) => {
                if type_path
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "GlobalEntity")
                {
                    return Some(
                        "WireSchema derive rejects GlobalEntity: it is intentionally \
                         unsupported (its manual serialization panics)",
                    );
                }
                for segment in &type_path.path.segments {
                    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                        continue;
                    };
                    for arg in &args.args {
                        let syn::GenericArgument::Type(inner) = arg else {
                            continue;
                        };
                        if let Some(hit) = reason(inner) {
                            return Some(hit);
                        }
                    }
                }
                None
            }
            Type::Tuple(tuple) => tuple.elems.iter().find_map(reason),
            Type::Array(array) => reason(&array.elem),
            Type::Slice(slice) => reason(&slice.elem),
            Type::Paren(paren) => reason(&paren.elem),
            Type::Group(group) => reason(&group.elem),
            _ => None,
        }
    }
    reason(ty).map(|message| quote! { compile_error!(#message); })
}

/// Rejects lifetime parameters: borrowed wire fields cannot be described,
/// so a type that names a lifetime has no honest descriptor.
pub(crate) fn reject_lifetimes(generics: &Generics) -> Option<TokenStream> {
    generics.lifetimes().next().map(|_| {
        quote! {
            compile_error!(
                "WireSchema derive rejects lifetimes: \
                 borrowed wire fields cannot be described"
            );
        }
    })
}

/// Derives `WireSchema` for a named-field struct from the same shape the
/// `Serde` derive serializes: field count plus each field's label and type
/// descriptor, in declaration order. Ordinary type parameters gain a
/// `WireSchema` bound; the original `where` clause survives untouched.
pub fn derive_wire_schema_struct(
    struct_: &DataStruct,
    struct_name: &Ident,
    generics: &Generics,
    schema_crate: &TokenStream,
) -> TokenStream {
    if let Some(rejection) = reject_lifetimes(generics) {
        return rejection;
    }
    let Fields::Named(fields) = &struct_.fields else {
        return quote! {
            compile_error!("WireSchema struct derive handles named fields only");
        };
    };

    let mut field_count = 0u32;
    let mut field_tokens = quote! {};
    for field in &fields.named {
        let Some(field_name) = field.ident.as_ref() else {
            continue;
        };
        if let Some(rejection) = reject_unsupported_field_type(&field.ty) {
            return rejection;
        }
        let field_label = field_name.to_string();
        let field_ty = &field.ty;
        field_count += 1;
        field_tokens = quote! {
            #field_tokens
            #schema_crate::wire_schema_label(out, #field_label);
            #schema_crate::wire_schema_field::<#field_ty>(ctx, out);
        };
    }

    let (impl_generics, ty_generics, where_clause) = wire_schema_generics(generics, schema_crate);

    // Flat emission with absolute paths: field types resolve exactly as the
    // user wrote them (a wrapper module would hide the parent scope and
    // break every non-fully-qualified field type).
    quote! {
        impl #impl_generics #schema_crate::WireSchema for #struct_name #ty_generics #where_clause {
            fn wire_schema(
                ctx: &mut #schema_crate::WireSchemaContext,
                out: &mut Vec<u8>,
            ) {
                out.push(#schema_crate::SCHEMA_TAG_STRUCT);
                #schema_crate::wire_schema_count(out, #field_count);
                #field_tokens
            }
        }
    }
}

/// Shared entry: emits `impl Serde` (historical) plus `impl WireSchema` from
/// the same shape, so the two can never drift. Both come from the one parsed
/// `DeriveInput` the caller hands over.
#[allow(clippy::format_push_string)]
pub fn derive_serde_struct(
    struct_: &DataStruct,
    struct_name: &Ident,
    generics: &Generics,
    serde_crate_name: TokenStream,
) -> TokenStream {
    let mut ser_body = quote! {};
    let mut de_body = quote! {};
    let mut bit_length_body = quote! {};

    for field in &struct_.fields {
        let field_name = field.ident.as_ref().expect("expected field to have a name");
        ser_body = quote! {
            #ser_body
            self.#field_name.ser(writer);
        };
        de_body = quote! {
            #de_body
            #field_name: Serde::de(reader)?,
        };
        bit_length_body = quote! {
            #bit_length_body
            output += self.#field_name.bit_length();
        };
    }

    let lowercase_struct_name = Ident::new(
        struct_name.to_string().to_lowercase().as_str(),
        Span::call_site(),
    );
    let module_name = format_ident!("define_serde_{}", lowercase_struct_name);

    let import_types = quote! { Serde, BitWrite, ConstBitLength, BitReader, SerdeErr };
    let imports = quote! { use #serde_crate_name::{#import_types}; };

    let schema_impl = derive_wire_schema_struct(struct_, struct_name, generics, &serde_crate_name);

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
