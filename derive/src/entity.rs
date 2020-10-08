use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, PathArguments, Type,
};

use super::utils;

pub fn entity_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let entity_name = &input.ident;
    let entity_builder_name = Ident::new(
        (entity_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let type_name = utils::get_type_name(&input, "Entity");

    let properties = utils::get_properties(&input);
    let interpolated_properties = get_interpolated_properties(&input);
    let predicted_properties = get_predicted_properties(&input);

    let enum_name = format_ident!("{}Prop", entity_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(entity_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&type_name, entity_name, &enum_name, &properties);
    let entity_write_method = utils::get_write_method(&properties);
    let entity_write_partial_method = get_write_partial_method(&enum_name, &properties);
    let entity_read_full_method = get_read_full_method(&properties);
    let entity_read_partial_method = get_read_partial_method(&enum_name, &properties);
    let set_mutator_method = get_set_mutator_method(&properties);
    let get_typed_copy_method = get_get_typed_copy_method(&type_name, entity_name, &properties);
    let equals_method = get_equals_method(entity_name, &properties);
    let equals_prediction_method = get_equals_prediction_method(entity_name, &predicted_properties);
    let set_to_interpolation_method =
        get_set_to_interpolation_method(entity_name, &properties, &interpolated_properties);
    let is_interpolated_method = get_is_interpolated_method(&predicted_properties);
    let is_predicted_method = get_is_predicted_method(&predicted_properties);
    let mirror_method = get_mirror_method(entity_name, &properties);

    let state_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{StateMask, EntityBuilder, EntityMutator, PropertyIo, EntityEq, interp_lerp};
        #property_enum
        pub struct #entity_builder_name {
            type_id: TypeId,
        }
        impl EntityBuilder<#type_name> for #entity_builder_name {
            fn get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn build(&self, buffer: &[u8]) -> #type_name {
                return #entity_name::read_to_type(buffer);
            }
        }
        impl #entity_name {
            pub fn get_builder() -> Box<dyn EntityBuilder<#type_name>> {
                return Box::new(#entity_builder_name {
                    type_id: TypeId::of::<#entity_name>(),
                });
            }
            pub fn wrap(self) -> Rc<RefCell<#entity_name>> {
                return Rc::new(RefCell::new(self));
            }
            #new_complete_method
            #read_to_type_method
        }
        impl Entity<#type_name> for #entity_name {
            fn get_state_mask_size(&self) -> u8 { #state_mask_size }
            fn get_type_id(&self) -> TypeId {
                return TypeId::of::<#entity_name>();
            }
            #set_mutator_method
            #entity_write_method
            #entity_write_partial_method
            #entity_read_full_method
            #entity_read_partial_method
            #get_typed_copy_method
            #is_interpolated_method
            #is_predicted_method
        }
        impl EntityEq<#type_name> for #entity_name {
            #equals_method
            #equals_prediction_method
            #set_to_interpolation_method
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
        fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
            #output
        }
    };
}

fn get_new_complete_method(
    entity_name: &Ident,
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
        pub fn new_complete(#args) -> #entity_name {
            #entity_name {
                #fields
            }
        }
    };
}

fn get_read_to_type_method(
    type_name: &Ident,
    entity_name: &Ident,
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
            #field_name.read(read_cursor);
        };
        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    return quote! {
        fn read_to_type(buffer: &[u8]) -> #type_name {
            let read_cursor = &mut Cursor::new(buffer);
            #prop_reads

            return #type_name::#entity_name(Rc::new(RefCell::new(#entity_name {
                #prop_names
            })));
        }
    };
}

fn get_get_typed_copy_method(
    type_name: &Ident,
    entity_name: &Ident,
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
            let copied_entity = #entity_name::new_complete(#args).wrap();
            return #type_name::#entity_name(copied_entity);
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
                PropertyIo::write(&self.#field_name, buffer);
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
            PropertyIo::read_seq(&mut self.#field_name, read_cursor, packet_index);
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn read_full(&mut self, buffer: &[u8], packet_index: u16) {
            let read_cursor = &mut Cursor::new(buffer);
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
                PropertyIo::read_seq(&mut self.#field_name, read_cursor, packet_index);
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8], packet_index: u16) {
            let read_cursor = &mut Cursor::new(buffer);
            #output
        }
    };
}

fn get_equals_method(entity_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn equals(&self, other: &#entity_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_equals_prediction_method(
    entity_name: &Ident,
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
        fn equals_prediction(&self, other: &#entity_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_set_to_interpolation_method(
    entity_name: &Ident,
    properties: &Vec<(Ident, Type)>,
    interpolated_properties: &Vec<(Ident, Type)>,
) -> TokenStream {
    let mut output = quote! {};

    for (field_name, field_type) in properties.iter() {
        let is_interpolated = {
            let mut i_output = false;
            for (interp_field_name, _) in interpolated_properties.iter() {
                if interp_field_name == field_name {
                    i_output = true;
                    break;
                }
            }
            i_output
        };

        let new_output_right = {
            if is_interpolated {
                quote! {
                    self.#field_name.set(interp_lerp::<#field_type>(old.#field_name.get(), new.#field_name.get(), fraction));
                }
            } else {
                quote! {
                    self.#field_name.mirror(&new.#field_name);
                }
            }
        };
        let new_output_result = quote! {
            #output
            #new_output_right
        };
        output = new_output_result;
    }

    return quote! {
        fn set_to_interpolation(&mut self, old: &#entity_name, new: &#entity_name, fraction: f32) {
            #output
        }
    };
}

fn get_mirror_method(entity_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn mirror(&mut self, other: &#entity_name) {
            #output
        }
    };
}

fn get_is_interpolated_method(properties: &Vec<(Ident, Type)>) -> TokenStream {
    let output = {
        if properties.len() > 0 {
            quote! { true }
        } else {
            quote! { false }
        }
    };

    return quote! {
        fn is_interpolated(&self) -> bool {
            return #output;
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

fn get_interpolated_properties(input: &DeriveInput) -> Vec<(Ident, Type)> {
    let mut fields: Vec<(Ident, Type)> = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in fields_named.named.iter() {
                for attr in field.attrs.iter() {
                    match attr.parse_meta().unwrap() {
                        syn::Meta::Path(ref path)
                            if path.get_ident().unwrap().to_string() == "interpolate" =>
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
