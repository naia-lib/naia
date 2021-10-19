use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};

use crate::replicate::{get_properties, get_protocol_path, get_property_enum, get_copy_method,
                    get_to_protocol_method, get_equals_method, get_mirror_method, get_write_method};

pub fn replicate_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let replica_name = &input.ident;
    let replica_builder_name = Ident::new(
        (replica_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let (protocol_path, protocol_name) = get_protocol_path(&input);
    let protocol_kind_name = format_ident!("{}Kind", protocol_name);

    let properties = get_properties(&input);

    let enum_name = format_ident!("{}Property", replica_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(replica_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&protocol_name, replica_name, &properties);
    let write_method = get_write_method(&properties);
    let read_partial_method = get_read_partial_method(&enum_name, &properties);
    let to_protocol_method = get_to_protocol_method(&protocol_name, replica_name);
    let copy_method = get_copy_method(replica_name);
    let equals_method = get_equals_method(replica_name, &properties);
    let mirror_method = get_mirror_method(replica_name, &properties);

    let gen = quote! {
        use std::{rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, ReplicaBuilder, PropertyMutate, ReplicateEq, PacketReader, Replicate};
        use #protocol_path::{#protocol_name, #protocol_kind_name};
        #property_enum
        pub struct #replica_builder_name {
            kind: #protocol_kind_name,
        }
        impl ReplicaBuilder<#protocol_name> for #replica_builder_name {
            fn get_kind(&self) -> #protocol_kind_name {
                return self.kind;
            }
            fn build(&self, reader: &mut PacketReader, packet_index: u16) -> #protocol_name {
                return #replica_name::read_to_type(reader, packet_index);
            }
        }
        impl #replica_name {
            pub fn get_builder() -> Box<dyn ReplicaBuilder<#protocol_name>> {
                return Box::new(#replica_builder_name {
                    kind: #replica_name::kind(),
                });
            }
            fn kind() -> #protocol_kind_name {
                return #protocol_kind_name::#replica_name;
            }
            #new_complete_method
            #read_to_type_method
        }
        impl Replicate<#protocol_name> for #replica_name {

            fn get_kind(&self) -> #protocol_kind_name {
                return #replica_name::kind();
            }
            #write_method
            #to_protocol_method

            #read_partial_method
        }
        impl ReplicateEq<#protocol_name> for #replica_name {
            #equals_method
            #mirror_method
            #copy_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_read_partial_method(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            if let Some(true) = diff_mask.get_bit(#enum_name::#uppercase_variant_name as u8) {
                Property::read(&mut self.#field_name, reader, packet_index);
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16) {
            #output
        }
    };
}

pub fn get_read_to_type_method(
    protocol_name: &Ident,
    replica_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut prop_names = quote! {};
    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            #field_name
        };
        let new_output_result = quote! {
            #prop_names
            #new_output_right,
        };
        prop_names = new_output_result;
    }

    let mut prop_reads = quote! {};
    for (field_name, field_type) in properties.iter() {
        let new_output_right = quote! {
            let mut #field_name = Property::<#field_type>::new_read(reader, packet_index);
        };
        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    return quote! {
        fn read_to_type(reader: &mut PacketReader, packet_index: u16) -> #protocol_name {
            #prop_reads

            return #protocol_name::#replica_name(#replica_name {
                #prop_names
            });
        }
    };
}

pub fn get_new_complete_method(
    replica_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut args = quote! {};
    for (field_name, field_type) in properties.iter() {
        let new_output_right = quote! {
            #field_name: #field_type
        };
        let new_output_result = quote! {
            #args#new_output_right,
        };
        args = new_output_result;
    }

    let mut fields = quote! {};
    for (field_name, field_type) in properties.iter() {
        let new_output_right = quote! {
            #field_name: Property::<#field_type>::new(#field_name)
        };
        let new_output_result = quote! {
            #fields
            #new_output_right,
        };
        fields = new_output_result;
    }

    return quote! {
        pub fn new_complete(#args) -> #replica_name {
            #replica_name {
                #fields
            }
        }
    };
}