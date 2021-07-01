use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, PathArguments, Type,
};

use super::utils;

pub fn actor_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let actor_name = &input.ident;
    let actor_builder_name = Ident::new(
        (actor_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let type_name = utils::get_type_name(&input, "Actor");

    let properties = utils::get_properties(&input);
    let predicted_properties = get_predicted_properties(&input);

    let enum_name = format_ident!("{}Prop", actor_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(actor_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&type_name, actor_name, &enum_name, &properties);
    let actor_write_method = utils::get_write_method(&properties);
    let actor_write_partial_method = get_write_partial_method(&enum_name, &properties);
    let actor_read_full_method = get_read_full_method(&properties);
    let actor_read_partial_method = get_read_partial_method(&enum_name, &properties);
    let set_mutator_method = get_set_mutator_method(&properties);
    let get_typed_copy_method = get_get_typed_copy_method(&type_name, actor_name, &properties);
    let equals_method = get_equals_method(actor_name, &properties);
    let equals_prediction_method = get_equals_prediction_method(actor_name, &predicted_properties);
    let is_predicted_method = get_is_predicted_method(&predicted_properties);
    let mirror_method = get_mirror_method(actor_name, &properties);

    let state_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell};
        use naia_shared::{StateMask, ActorBuilder, ActorMutator, ActorEq, PacketReader, Ref};
        #property_enum
        pub struct #actor_builder_name {
            type_id: TypeId,
        }
        impl ActorBuilder<#type_name> for #actor_builder_name {
            fn get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn build(&self, reader: &mut PacketReader) -> #type_name {
                return #actor_name::read_to_type(reader);
            }
        }
        impl #actor_name {
            pub fn get_builder() -> Box<dyn ActorBuilder<#type_name>> {
                return Box::new(#actor_builder_name {
                    type_id: TypeId::of::<#actor_name>(),
                });
            }
            pub fn wrap(self) -> Ref<#actor_name> {
                return Ref::new(self);
            }
            #new_complete_method
            #read_to_type_method
        }
        impl Actor<#type_name> for #actor_name {
            fn get_state_mask_size(&self) -> u8 { #state_mask_size }
            fn get_type_id(&self) -> TypeId {
                return TypeId::of::<#actor_name>();
            }
            #set_mutator_method
            #actor_write_method
            #actor_write_partial_method
            #actor_read_full_method
            #actor_read_partial_method
            #get_typed_copy_method
            #is_predicted_method
        }
        impl ActorEq<#type_name> for #actor_name {
            #equals_method
            #equals_prediction_method
            #mirror_method
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
        fn set_mutator(&mut self, mutator: &Ref<dyn ActorMutator>) {
            #output
        }
    };
}

fn get_new_complete_method(
    actor_name: &Ident,
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
        pub fn new_complete(#args) -> #actor_name {
            #actor_name {
                #fields
            }
        }
    };
}

fn get_read_to_type_method(
    type_name: &Ident,
    actor_name: &Ident,
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

            return #type_name::#actor_name(Ref::new(#actor_name {
                #prop_names
            }));
        }
    };
}

fn get_get_typed_copy_method(
    type_name: &Ident,
    actor_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut args = quote! {};
    for (field_name, _) in properties.iter() {
        let new_output_right = quote! {
            self.#field_name.get().clone()
        };
        let new_output_result = quote! {
            #args#new_output_right,
        };
        args = new_output_result;
    }

    return quote! {
        fn get_typed_copy(&self) -> #type_name {
            let copied_actor = #actor_name::new_complete(#args).wrap();
            return #type_name::#actor_name(copied_actor);
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
            if let Some(true) = state_mask.get_bit(#enum_name::#uppercase_variant_name as u8) {
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
        fn write_partial(&self, state_mask: &StateMask, buffer: &mut Vec<u8>) {

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
            if let Some(true) = state_mask.get_bit(#enum_name::#uppercase_variant_name as u8) {
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
        fn read_partial(&mut self, state_mask: &StateMask, reader: &mut PacketReader, packet_index: u16) {
            #output
        }
    };
}

fn get_equals_method(actor_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn equals(&self, other: &#actor_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_equals_prediction_method(
    actor_name: &Ident,
    properties: &Vec<(Ident, Type)>,
) -> TokenStream {
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
        fn equals_prediction(&self, other: &#actor_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_mirror_method(actor_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn mirror(&mut self, other: &#actor_name) {
            #output
        }
    };
}

fn get_is_predicted_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let output = {
        if properties.len() > 0 {
            quote! { true }
        } else {
            quote! { false }
        }
    };

    return quote! {
        fn is_predicted(&self) -> bool {
            return #output;
        }
    };
}

fn get_predicted_properties(input: &DeriveInput) -> Vec<(Ident, Type)> {
    let mut fields: Vec<(Ident, Type)> = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in fields_named.named.iter() {
                for attr in field.attrs.iter() {
                    match attr.parse_meta().unwrap() {
                        syn::Meta::Path(ref path)
                            if path.get_ident().unwrap().to_string() == "predict" =>
                        {
                            if let Some(property_name) = &field.ident {
                                if let Type::Path(type_path) = &field.ty {
                                    if let PathArguments::AngleBracketed(angle_args) =
                                        &type_path.path.segments.first().unwrap().arguments
                                    {
                                        if let Some(GenericArgument::Type(property_type)) =
                                            angle_args.args.first()
                                        {
                                            fields.push((
                                                property_name.clone(),
                                                (*property_type).clone(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fields
}
