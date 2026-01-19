use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, LitStr};

use super::shared::{get_struct_type, StructType};

pub fn channel_impl(
    input: proc_macro::TokenStream,
    shared_crate_name: TokenStream,
) -> proc_macro::TokenStream {
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
    let struct_name_str = LitStr::new(&struct_name.to_string(), struct_name.span());

    let gen = quote! {
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
    };

    proc_macro::TokenStream::from(gen)
}
