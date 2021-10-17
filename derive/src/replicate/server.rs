use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};

use super::shared::{get_properties, get_protocol_path, get_property_enum, get_new_complete_method, get_read_to_type_method};

pub fn replicate_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let replica_name = &input.ident;
    let replica_builder_name = Ident::new(
        (replica_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let protocol_name = get_protocol_name(&input);

    let properties = get_properties(&input);

    let enum_name = format_ident!("{}Property", replica_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(replica_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&protocol_name, replica_name, &enum_name, &properties);
    let write_method = get_write_method(&properties);
    let write_partial_method = get_write_partial_method(&enum_name, &properties);
    let read_full_method = get_read_full_method(&properties);
    let read_partial_method = get_read_partial_method(&enum_name, &properties);
    let set_mutator_method = get_set_mutator_method(&properties);
    let to_protocol_method = get_to_protocol_method(&protocol_name, replica_name);
    let copy_method = get_copy_method(replica_name);
    let equals_method = get_equals_method(replica_name, &properties);
    let mirror_method = get_mirror_method(replica_name, &properties);

    let diff_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, ReplicaBuilder, PropertyMutate, ReplicateEq, PacketReader, Replicate};
        #property_enum
        pub struct #replica_builder_name {
            type_id: TypeId,
        }
        impl ReplicaBuilder<#protocol_name> for #replica_builder_name {
            fn get_kind(&self) -> TypeId {
                return self.type_id;
            }
            fn build(&self, reader: &mut PacketReader) -> #protocol_name {
                return #replica_name::read_to_type(reader);
            }
        }
        impl #replica_name {
            pub fn get_builder() -> Box<dyn ReplicaBuilder<#protocol_name>> {
                return Box::new(#replica_builder_name {
                    type_id: TypeId::of::<#replica_name>(),
                });
            }
            #new_complete_method
            #read_to_type_method
        }
        impl Replicate<#protocol_name> for #replica_name {
            fn get_diff_mask_size(&self) -> u8 { #diff_mask_size }
            fn get_kind(&self) -> TypeId {
                return TypeId::of::<#replica_name>();
            }
            #set_mutator_method
            #write_method
            #write_partial_method
            #read_full_method
            #read_partial_method
            #to_protocol_method
        }
        impl ReplicateEq<#protocol_name> for #replica_name {
            #equals_method
            #mirror_method
            #copy_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_property_enum(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let hashtag = Punct::new('#', Spacing::Alone);

    let mut variant_index: u8 = 0;
    let mut variant_list = quote! {};
    for (variant, _) in properties {
        let uppercase_variant_name = Ident::new(
            variant.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            #uppercase_variant_name = #variant_index,
        };
        let new_output_result = quote! {
            #variant_list
            #new_output_right
        };
        variant_list = new_output_result;

        variant_index += 1;
    }

    return quote! {
        #hashtag[repr(u8)]
        enum #enum_name {
            #variant_list
        }
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
        fn set_mutator(&mut self, mutator: &Ref<dyn PropertyMutate>) {
            #output
        }
    };
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
        fn to_protocol(&self) -> #protocol_name {
            let copied_replica = self.clone().to_ref();
            return #protocol_name::#replica_name(copied_replica);
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

use syn::{Data, Fields, GenericArgument, Lit, Meta, PathArguments};

fn get_properties(input: &DeriveInput) -> Vec<(Ident, Type)> {
    let mut fields = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in fields_named.named.iter() {
                if let Some(property_name) = &field.ident {
                    if let Type::Path(type_path) = &field.ty {
                        if let PathArguments::AngleBracketed(angle_args) =
                            &type_path.path.segments.first().unwrap().arguments
                        {
                            if let Some(GenericArgument::Type(property_type)) =
                                angle_args.args.first()
                            {
                                fields.push((property_name.clone(), property_type.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    fields
}

fn get_protocol_name(input: &DeriveInput) -> Ident {
    let mut protocol_name_option: Option<Ident> = None;

    let attrs = &input.attrs;
    for option in attrs.into_iter() {
        let option = option.parse_meta().unwrap();
        match option {
            Meta::NameValue(meta_name_value) => {
                let path = meta_name_value.path;
                let lit = meta_name_value.lit;
                if let Some(ident) = path.get_ident() {
                    if ident == "protocol_name" {
                        if let Lit::Str(lit) = lit {
                            let ident = Ident::new(lit.value().as_str(), Span::call_site());
                            protocol_name_option = Some(ident);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if protocol_name_option.is_none() {
        return Ident::new("Protocol", Span::call_site());
    } else {
        return protocol_name_option.unwrap();
    }
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