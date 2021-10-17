use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};

pub fn replicate_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let replica_name = &input.ident;
    let replica_builder_name = Ident::new(
        (replica_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let type_name = get_type_name(&input);

    let properties = get_properties(&input);

    let enum_name = format_ident!("{}Prop", replica_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(replica_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&type_name, replica_name, &enum_name, &properties);
    let write_method = get_write_method(&properties);
    let write_partial_method = get_write_partial_method(&enum_name, &properties);
    let read_full_method = get_read_full_method(&properties);
    let read_partial_method = get_read_partial_method(&enum_name, &properties);
    let set_mutator_method = get_set_mutator_method(&properties);
    let copy_to_protocol_method = get_copy_to_protocol_method(&type_name, replica_name);
    let copy_method = get_copy_method(replica_name);
    let equals_method = get_equals_method(replica_name, &properties);
    let mirror_method = get_mirror_method(replica_name, &properties);

    let convert_method_name = Ident::new(
        (replica_name.to_string() + "Convert").as_str(),
        Span::call_site(),
    );

    let diff_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, ReplicaBuilder, PropertyMutate, PacketReader, Ref, Replicate};
        #property_enum
        pub struct #replica_builder_name {
            type_id: TypeId,
        }
        impl ReplicaBuilder<#type_name> for #replica_builder_name {
            fn get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn build(&self, reader: &mut PacketReader) -> #type_name {
                return #replica_name::read_to_type(reader);
            }
        }
        impl #replica_name {
            pub fn get_builder() -> Box<dyn ReplicaBuilder<#type_name>> {
                return Box::new(#replica_builder_name {
                    type_id: TypeId::of::<#replica_name>(),
                });
            }
            pub fn to_ref(self) -> Ref<#replica_name> {
                return Ref::new(self);
            }
            #new_complete_method
            #read_to_type_method
        }
//        impl ImplRef<#type_name> for Ref<#replica_name> {
//            fn protocol(&self) -> #type_name {
//                return #type_name::#replica_name(self.clone());
//            }
//            fn dyn_ref(&self) -> Ref<dyn Replicate<#type_name>> {
//                let upcast_ref: Ref<dyn Replicate<#type_name>> = #type_name::#convert_method_name(self.clone());
//                return upcast_ref.clone();
//            }
//            fn clone_ref(&self) -> Ref<#replica_name> {
//                return self.clone();
//            }
//        }
        impl Replicate<#type_name> for #replica_name {
            fn get_diff_mask_size(&self) -> u8 { #diff_mask_size }
            fn get_type_id(&self) -> TypeId {
                return TypeId::of::<#replica_name>();
            }
            #set_mutator_method
            #write_method
            #write_partial_method
            #read_full_method
            #read_partial_method
            #copy_to_protocol_method
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

fn get_new_complete_method(
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
        pub fn new_complete(#args) -> Ref<#replica_name> {
            Ref::new(#replica_name {
                #fields
            })
        }
    };
}

fn get_read_to_type_method(
    type_name: &Ident,
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
            let mut #field_name = Property::<#field_type>::new(Default::default(), #enum_name::#uppercase_variant_name as u8);
            #field_name.read(reader, 1);
        };
        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    return quote! {
        fn read_to_type(reader: &mut PacketReader) -> #type_name {
            #prop_reads

            return #type_name::#replica_name(Ref::new(#replica_name {
                #prop_names
            }));
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

fn get_copy_to_protocol_method(type_name: &Ident, replica_name: &Ident) -> TokenStream {
    return quote! {
        fn copy_to_protocol(&self) -> #type_name {
            let copied_replica = self.clone().to_ref();
            return #type_name::#replica_name(copied_replica);
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

fn get_type_name(input: &DeriveInput) -> Ident {
    let mut type_name_option: Option<Ident> = None;

    let attrs = &input.attrs;
    for option in attrs.into_iter() {
        let option = option.parse_meta().unwrap();
        match option {
            Meta::NameValue(meta_name_value) => {
                let path = meta_name_value.path;
                let lit = meta_name_value.lit;
                if let Some(ident) = path.get_ident() {
                    if ident == "type_name" {
                        if let Lit::Str(lit) = lit {
                            let ident = Ident::new(lit.value().as_str(), Span::call_site());
                            type_name_option = Some(ident);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if type_name_option.is_none() {
        return Ident::new("Protocol", Span::call_site());
    } else {
        return type_name_option.unwrap();
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
