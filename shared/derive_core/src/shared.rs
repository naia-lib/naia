use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericParam, Generics};

pub enum StructType {
    Struct,
    UnitStruct,
    TupleStruct,
}

/// Get the type of the struct
pub(crate) fn get_struct_type(input: &DeriveInput) -> StructType {
    if let Data::Struct(data_struct) = &input.data {
        return match &data_struct.fields {
            Fields::Named(_) => StructType::Struct,
            Fields::Unnamed(_) => StructType::TupleStruct,
            Fields::Unit => StructType::UnitStruct,
        };
    }
    panic!("Can only derive on a struct")
}

#[must_use]
pub fn get_generics(input: &DeriveInput) -> (TokenStream, TokenStream, TokenStream) {
    let generics = &input.generics;
    if generics.lt_token.is_none() {
        return (quote! {}, quote! {}, quote! {});
    }

    let (impl_generics, ty_generics, _) = generics.split_for_impl();
    let typed_generics = quote! {
        #impl_generics
    };
    let untyped_generics = quote! {
        #ty_generics
    };
    let turbofish = ty_generics.as_turbofish();
    (untyped_generics, typed_generics, quote! { #turbofish })
}

/// Builds the `{ phantom_T: PhantomData<T>, .. }` tail for a builder struct.
///
/// # Panics
///
/// Panics if any generic parameter is not a plain type parameter.
#[must_use]
pub fn get_builder_generic_fields(generics: &Generics) -> TokenStream {
    if generics.gt_token.is_none() {
        return quote! { ; };
    }

    let mut output = quote! {};

    for param in &generics.params {
        let GenericParam::Type(type_param) = param else {
            panic!("Only type parameters are supported for now");
        };

        let uppercase_letter = &type_param.ident;
        let field_name = format_ident!("phantom_{}", type_param.ident.to_string().to_lowercase());
        let field_tokens = quote! {
            #field_name: std::marker::PhantomData<#uppercase_letter>,
        };
        let new_output_result = quote! {
            #output
            #field_tokens
        };
        output = new_output_result;
    }

    quote! {
        { #output }
    }
}
