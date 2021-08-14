//use proc_macro2::TokenStream;
//use quote::quote;
//use syn::{parse_macro_input, Data, DeriveInput, Ident};
//
//pub fn event_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
//    let input = parse_macro_input!(input as DeriveInput);
//
//    let type_name = input.ident;
//
//
//
//    let gen = quote! {
//        use std::any::TypeId;
//        use naia_shared::{EventType, Event};
//        impl EventType for #type_name {
//            fn event_write(&self, buffer: &mut Vec<u8>) {
//                match self {
//                    #write_variants
//                }
//            }
//            fn event_get_type_id(&self) -> TypeId {
//                match self {
//                    #get_type_id_variants
//                }
//            }
//        }
//    };
//
//    proc_macro::TokenStream::from(gen)
//}
//
//
