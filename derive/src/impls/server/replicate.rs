use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Type};

pub fn get_builder_impl_methods(replica_name: &Ident, protocol_name: &Ident) -> TokenStream {
    return quote! {
        fn build(&self, reader: &mut PacketReader, _packet_index: u16) -> #protocol_name {
            return #replica_name::read_to_type(reader);
        }
    };
}

pub fn get_replica_impl_methods(protocol_name: &Ident,
                                replica_name: &Ident,
                                enum_name: &Ident,
                                properties: &Vec<(Ident, Type)>) -> TokenStream {
    let new_complete_method = get_new_complete_method(replica_name, enum_name, properties);
    let read_to_type_method = get_read_to_type_method(protocol_name, replica_name, enum_name, properties);

    return quote! {
        #new_complete_method
        #read_to_type_method
    };
}

pub fn get_replicate_derive_methods(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let write_partial_method = get_write_partial_method(enum_name, properties);
    let set_mutator_method = get_set_mutator_method(properties);
    let diff_mask_size = (((properties.len() - 1) / 8) + 1) as u8;

    return quote! {
        fn get_diff_mask_size(&self) -> u8 { #diff_mask_size }
        #write_partial_method
        #set_mutator_method
    };
}

fn get_set_mutator_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            self.#field_name.set_mutator(mutator);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn set_mutator(&mut self, mutator: &PropertyMutator) {
            #output
        }
    };
}

fn get_write_partial_method(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            if let Some(true) = diff_mask.get_bit(#enum_name::#uppercase_variant_name as u8) {
                Property::write(&self.#field_name, buffer);
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn write_partial(&self, diff_mask: &DiffMask, buffer: &mut Vec<u8>) {

            #output
        }
    };
}

pub fn get_new_complete_method(
    replica_name: &Ident,
    enum_name: &Ident,
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
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            #field_name: Property::<#field_type>::new(#field_name, #enum_name::#uppercase_variant_name as u8)
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

pub fn get_read_to_type_method(
    protocol_name: &Ident,
    replica_name: &Ident,
    enum_name: &Ident,
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
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            let #field_name = Property::<#field_type>::new_read(reader, #enum_name::#uppercase_variant_name as u8);
        };
        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    return quote! {
        fn read_to_type(reader: &mut PacketReader) -> #protocol_name {
            #prop_reads

            return #protocol_name::#replica_name(#replica_name {
                #prop_names
            });
        }
    };
}