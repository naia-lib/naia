use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitStr};

use super::shared::{get_struct_type, StructType};

/// Derives the `Channel` marker plus `Named` for a unit struct.
///
/// # Panics
///
/// Panics if the input is not a unit struct.
#[must_use]
pub fn channel_impl(input: &DeriveInput, shared_crate_name: &TokenStream) -> TokenStream {
    // Helper Properties
    let struct_type = get_struct_type(input);
    match struct_type {
        StructType::Struct | StructType::TupleStruct => {
            panic!("Can only derive Channel on a Unit struct (i.e. `struct MyStruct;`)");
        }
        StructType::UnitStruct => {}
    }

    // Names
    let struct_name = input.ident.clone();
    let struct_name_str = LitStr::new(&struct_name.to_string(), struct_name.span());

    quote! {
        impl #shared_crate_name::Channel for #struct_name {

        }

        impl #shared_crate_name::Named for #struct_name {
            fn name(&self) -> String {
                #struct_name_str.to_string()
            }
            fn protocol_name() -> &'static str {
                #struct_name_str
            }
        }
    }
}
