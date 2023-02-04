use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, Index, Lit, LitStr,
    Member, Meta, Path, PathArguments, Result, Type,
};

use super::{shared::{get_struct_type, StructType}};

pub fn channel_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Helper Properties
    let struct_type = get_struct_type(&input);
    match struct_type {
        StructType::Struct | StructType::TupleStruct => {
            panic!("Can only derive Channel on a Unit struct (i.e. `struct MyStruct;`)");
        }
        _ => {}
    }

    // Names
    let struct_name = input.ident;

    let gen = quote! {

        impl Channel for #struct_name {

        }
    };

    proc_macro::TokenStream::from(gen)
}