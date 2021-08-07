use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn event_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let write_variants = get_write_variants(&type_name, &input.data);
    let get_type_id_variants = get_type_id_variants(&type_name, &input.data);

    let gen = quote! {
        use std::any::TypeId;
        use naia_shared::{EventType, Event};
        impl EventType for #type_name {
            fn event_write(&self, buffer: &mut Vec<u8>) {
                match self {
                    #write_variants
                }
            }
            fn event_get_type_id(&self) -> TypeId {
                match self {
                    #get_type_id_variants
                }
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_write_variants(type_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idstate) => {
                        idstate.event_write(buffer);
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    }
}

fn get_type_id_variants(type_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idstate) => {
                        return idstate.event_get_type_id();
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    }
}
