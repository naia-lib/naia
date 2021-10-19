use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};
use syn::{Data, Fields, GenericArgument, Lit, Meta, PathArguments, Path, Result};

use crate::impls::replicate::*;

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

    let write_method = get_write_method(&properties);
    let to_protocol_method = get_to_protocol_method(&protocol_name, replica_name);
    let copy_method = get_copy_method(replica_name);
    let equals_method = get_equals_method(replica_name, &properties);
    let mirror_method = get_mirror_method(replica_name, &properties);

    let builder_impl_methods = get_builder_impl_methods(replica_name, &protocol_name);
    let replica_impl_methods = get_replica_impl_methods(&protocol_name, replica_name, &enum_name, &properties);
    let replicate_derive_methods = get_replicate_derive_methods(&enum_name, &properties);

    let gen = quote! {
        use std::{rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, ReplicaBuilder, PropertyMutate, ReplicateEq, PacketReader, Replicate, PropertyMutator, ProtocolType};
        use #protocol_path::{#protocol_name, #protocol_kind_name};
        #property_enum
        pub struct #replica_builder_name {
            kind: #protocol_kind_name,
        }
        impl ReplicaBuilder<#protocol_name> for #replica_builder_name {
            fn get_kind(&self) -> #protocol_kind_name {
                return self.kind;
            }
            #builder_impl_methods
        }
        impl #replica_name {
            pub fn get_builder() -> Box<dyn ReplicaBuilder<#protocol_name>> {
                return Box::new(#replica_builder_name {
                    kind: ProtocolType::kind_of::<#replica_name>(),
                });
            }
            #replica_impl_methods
        }
        impl Replicate<#protocol_name> for #replica_name {

            fn get_kind(&self) -> #protocol_kind_name {
                return ProtocolType::kind_of::<Self>();
            }
            #write_method
            #to_protocol_method

            #replicate_derive_methods
        }
        impl ReplicateEq<#protocol_name> for #replica_name {
            #equals_method
            #mirror_method
            #copy_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

pub fn get_properties(input: &DeriveInput) -> Vec<(Ident, Type)> {
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

pub fn get_protocol_path(input: &DeriveInput) -> (Path, Ident) {
    let mut path_result: Option<Result<Path>> = None;

    let attrs = &input.attrs;
    for option in attrs.into_iter() {
        let option = option.parse_meta().unwrap();
        match option {
            Meta::NameValue(meta_name_value) => {
                let path = meta_name_value.path;
                let lit = meta_name_value.lit;
                if let Some(ident) = path.get_ident() {
                    if ident == "protocol_path" {
                        if let Lit::Str(lit_str) = lit {
                            path_result = Some(lit_str.parse());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(Ok(path)) = path_result {
        let mut new_path = path.clone();
        if let Some(last_seg) = new_path.segments.pop() {
            let name = last_seg.into_value().ident;
            if let Some(second_seg) = new_path.segments.pop() {
                new_path.segments.push_value(second_seg.into_value());
                return (new_path, name);
            }
        }

    }

    panic!("When deriving 'Replicate' you MUST specify the path of the accompanying protocol. IE: '#[protocol_path = \"crate::MyProtocol\"]'");
}

pub fn get_property_enum(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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

pub fn get_copy_method(replica_name: &Ident) -> TokenStream {
    return quote! {
        fn copy(&self) -> #replica_name {
            return self.clone();
        }
    };
}

pub fn get_to_protocol_method(protocol_name: &Ident, replica_name: &Ident) -> TokenStream {
    return quote! {
        fn to_protocol(self) -> #protocol_name {
            return #protocol_name::#replica_name(self);
        }
    };
}

pub fn get_equals_method(replica_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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

pub fn get_mirror_method(replica_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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

pub fn get_write_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
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