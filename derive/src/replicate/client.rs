use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};
use syn::{Data, Fields, GenericArgument, Lit, Meta, PathArguments};

use super::shared::{get_properties, get_protocol_path, get_property_enum, get_new_complete_method, get_read_to_type_method};

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

    let new_complete_method = get_new_complete_method(replica_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&protocol_name, replica_name, &enum_name, &properties);
    let write_method = get_write_method(&properties);
    let read_full_method = get_read_full_method(&properties);
    let read_partial_method = get_read_partial_method(&enum_name, &properties);
    let to_protocol_method = get_to_protocol_method(&protocol_name, replica_name);
    let copy_method = get_copy_method(replica_name);
    let equals_method = get_equals_method(replica_name, &properties);
    let mirror_method = get_mirror_method(replica_name, &properties);

    let diff_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell, io::Cursor};
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
            fn build(&self, reader: &mut PacketReader) -> #protocol_name {
                return #replica_name::read_to_type(reader);
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
        impl Replicate for #replica_name {
            type Protocol = #protocol_name;
            type Kind = #protocol_kind_name;

            fn get_kind(&self) -> #protocol_kind_name {
                return #replica_name::kind();
            }
            #write_method
            #read_full_method
            #read_partial_method
            #to_protocol_method
        }
        impl ReplicateEq for #replica_name {
            #equals_method
            #mirror_method
            #copy_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_copy_method(replica_name: &Ident) -> TokenStream {
    return quote! {
        fn copy(&self) -> #replica_name {
            return self.clone();
        }
    };
}

fn get_to_protocol_method(protocol_name: &Ident, replica_name: &Ident) -> TokenStream {
    return quote! {
        fn to_protocol(self) -> #protocol_name {
            return #protocol_name::#replica_name(self);
        }
    };
}

fn get_read_full_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            Property::read(&mut self.#field_name, reader, packet_index);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16) {
            #output
        }
    };
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

fn get_equals_method(replica_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            if !Property::equals(&self.#field_name, &other.#field_name) { return false; }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn equals(&self, other: &#replica_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_mirror_method(replica_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            self.#field_name.mirror(&other.#field_name);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn mirror(&mut self, other: &#replica_name) {
            #output
        }
    };
}



fn get_write_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            Property::write(&self.#field_name, buffer);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn write(&self, buffer: &mut Vec<u8>) {
            #output
        }
    };
}