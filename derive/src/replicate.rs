use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, Lit, Meta, Path,
    PathArguments, Result, Type,
};

pub fn replicate_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Helper Properties
    let properties = properties(&input);

    // Paths
    let (protocol_path, protocol_name) = protocol_path(&input);

    // Names
    let replica_name = input.ident;
    let replica_builder_name = Ident::new(
        (replica_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let protocol_kind_name = format_ident!("{}Kind", protocol_name);
    let enum_name = format_ident!("{}Property", replica_name);

    // Definitions
    let property_enum_definition = property_enum(&enum_name, &properties);

    // Replica Methods
    let new_complete_method = new_complete_method(&replica_name, &enum_name, &properties);
    let read_to_type_method =
        read_to_type_method(&protocol_name, &replica_name, &enum_name, &properties);

    // ReplicateSafe Derive Methods
    let diff_mask_size = (((properties.len() - 1) / 8) + 1) as u8;
    let dyn_ref_method = dyn_ref_method(&protocol_name);
    let dyn_mut_method = dyn_mut_method(&protocol_name);
    let to_protocol_method = into_protocol_method(&protocol_name, &replica_name);
    let protocol_copy_method = protocol_copy_method(&protocol_name, &replica_name);
    let clone_method = clone_method(&replica_name, &properties);
    let mirror_method = mirror_method(&protocol_name, &replica_name, &properties);
    let set_mutator_method = set_mutator_method(&properties);
    let read_partial_method = read_partial_method(&enum_name, &properties);
    let write_method = write_method(&properties);
    let write_partial_method = write_partial_method(&enum_name, &properties);
    let size_method = size_method(&properties);
    let size_partial_method = size_partial_method(&enum_name, &properties);

    let gen = quote! {
        use std::{rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, ReplicaBuilder, PropertyMutate, PacketReader, Replicate, ReplicateSafe, PropertyMutator, Protocolize, ReplicaDynRef, ReplicaDynMut};
        use #protocol_path::{#protocol_name, #protocol_kind_name};
        #property_enum_definition
        pub struct #replica_builder_name {
            kind: #protocol_kind_name,
        }
        impl ReplicaBuilder<#protocol_name> for #replica_builder_name {
            fn kind(&self) -> #protocol_kind_name {
                return self.kind;
            }
            fn build(&self, reader: &mut PacketReader) -> #protocol_name {
                return #replica_name::read_to_type(reader);
            }
        }
        impl #replica_name {
            pub fn builder() -> Box<dyn ReplicaBuilder<#protocol_name>> {
                return Box::new(#replica_builder_name {
                    kind: Protocolize::kind_of::<#replica_name>(),
                });
            }
            #new_complete_method
            #read_to_type_method
            #size_method
            #size_partial_method
        }
        impl ReplicateSafe<#protocol_name> for #replica_name {
            fn diff_mask_size(&self) -> u8 { #diff_mask_size }
            fn kind(&self) -> #protocol_kind_name {
                return Protocolize::kind_of::<Self>();
            }
            #dyn_ref_method
            #dyn_mut_method
            #to_protocol_method
            #protocol_copy_method
            #mirror_method
            #set_mutator_method
            #read_partial_method
            #write_method
            #write_partial_method
        }
        impl Replicate<#protocol_name> for #replica_name {
            #clone_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn properties(input: &DeriveInput) -> Vec<(Ident, Type)> {
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

fn protocol_path(input: &DeriveInput) -> (Path, Ident) {
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

fn property_enum(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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

fn protocol_copy_method(protocol_name: &Ident, replica_name: &Ident) -> TokenStream {
    return quote! {
        fn protocol_copy(&self) -> #protocol_name {
            return #protocol_name::#replica_name(self.clone());
        }
    };
}

fn into_protocol_method(protocol_name: &Ident, replica_name: &Ident) -> TokenStream {
    return quote! {
        fn into_protocol(self) -> #protocol_name {
            return #protocol_name::#replica_name(self);
        }
    };
}

pub fn dyn_ref_method(protocol_name: &Ident) -> TokenStream {
    return quote! {
        fn dyn_ref(&self) -> ReplicaDynRef<'_, #protocol_name> {
            return ReplicaDynRef::new(self);
        }
    };
}

pub fn dyn_mut_method(protocol_name: &Ident) -> TokenStream {
    return quote! {
        fn dyn_mut(&mut self) -> ReplicaDynMut<'_, #protocol_name> {
            return ReplicaDynMut::new(self);
        }
    };
}

fn clone_method(replica_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            self.#field_name.get().clone(),
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn clone(&self) -> #replica_name {
            return #replica_name::new_complete(#output);
        }
    };
}

fn mirror_method(
    protocol_name: &Ident,
    replica_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            self.#field_name.mirror(&replica.#field_name);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn mirror(&mut self, other: &#protocol_name) {
            if let #protocol_name::#replica_name(replica) = other {
                #output
            }
        }
    };
}

fn set_mutator_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
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

pub fn new_complete_method(
    replica_name: &Ident,
    enum_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut args = quote! {};
    for (field_name, field_type) in properties.iter() {
        let new_output_right = quote! {
            #field_name: #field_type,
        };
        let new_output_result = quote! {
            #args #new_output_right
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

pub fn read_to_type_method(
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

fn read_partial_method(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            if let Some(true) = diff_mask.bit(#enum_name::#uppercase_variant_name as u8) {
                Property::read(&mut self.#field_name, reader);
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader) {
            #output
        }
    };
}

fn write_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
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

fn write_partial_method(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, _) in properties.iter() {
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            if let Some(true) = diff_mask.bit(#enum_name::#uppercase_variant_name as u8) {
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

fn size_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (_, field_type) in properties.iter() {
        let new_output_right = quote! {
            size += Property::<#field_type>::size();
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        pub fn size() -> usize {
            let mut size = 0;
            #output
            size
        }
    };
}

fn size_partial_method(enum_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
    let mut output = quote! {};

    for (field_name, field_type) in properties.iter() {
        let uppercase_variant_name = Ident::new(
            field_name.to_string().to_uppercase().as_str(),
            Span::call_site(),
        );

        let new_output_right = quote! {
            if let Some(true) = diff_mask.bit(#enum_name::#uppercase_variant_name as u8) {
                size += Property::<#field_type>::size();
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        pub fn size_partial(diff_mask: &DiffMask) -> usize {
            let mut size = 0;
            #output
            size
        }
    };
}