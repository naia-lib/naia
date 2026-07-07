use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitStr};

use super::shared::{get_struct_type, StructType};

pub fn channel_impl(input: DeriveInput, shared_crate_name: TokenStream) -> TokenStream {
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
