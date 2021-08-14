use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, DeriveInput, Ident, Type,
};

pub fn state_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let state_name = &input.ident;
    let state_builder_name = Ident::new(
        (state_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );
    let type_name = get_type_name(&input, "State");

    let properties = get_properties(&input);

    let enum_name = format_ident!("{}Prop", state_name);
    let property_enum = get_property_enum(&enum_name, &properties);

    let new_complete_method = get_new_complete_method(state_name, &enum_name, &properties);
    let read_to_type_method =
        get_read_to_type_method(&type_name, state_name, &enum_name, &properties);
    let state_write_method = get_write_method(&properties);
    let state_write_partial_method = get_write_partial_method(&enum_name, &properties);
    let state_read_full_method = get_read_full_method(&properties);
    let state_read_partial_method = get_read_partial_method(&enum_name, &properties);
    let set_mutator_method = get_set_mutator_method(&properties);
    let get_typed_copy_method = get_get_typed_copy_method(&type_name, state_name, &properties);
    let equals_method = get_equals_method(state_name, &properties);
    let mirror_method = get_mirror_method(state_name, &properties);

    let diff_mask_size: u8 = (((properties.len() - 1) / 8) + 1) as u8;

    let gen = quote! {
        use std::{any::{TypeId}, rc::Rc, cell::RefCell, io::Cursor};
        use naia_shared::{DiffMask, StateBuilder, StateMutator, StateEq, PacketReader, Ref, EventBuilder};
        #property_enum
        pub struct #state_builder_name {
            type_id: TypeId,
        }
        impl StateBuilder<#type_name> for #state_builder_name {
            fn state_get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn state_build(&self, reader: &mut PacketReader) -> #type_name {
                return #state_name::state_read_to_type(reader);
            }
        }
        impl #state_name {
            pub fn state_get_builder() -> Box<dyn StateBuilder<#type_name>> {
                return Box::new(#state_builder_name {
                    type_id: TypeId::of::<#state_name>(),
                });
            }
            pub fn state_wrap(self) -> Ref<#state_name> {
                return Ref::new(self);
            }
            #new_complete_method
            #read_to_type_method
        }
        impl State<#type_name> for #state_name {
            fn event_is_guaranteed(&self) -> bool {
                #state_name::is_guaranteed()
            }
            fn state_get_diff_mask_size(&self) -> u8 { #diff_mask_size }
            fn state_get_type_id(&self) -> TypeId {
                return TypeId::of::<#state_name>();
            }
            #set_mutator_method
            #state_write_method
            #state_write_partial_method
            #state_read_full_method
            #state_read_partial_method
            #get_typed_copy_method
        }
        impl StateEq<#type_name> for #state_name {
            #equals_method
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
        fn state_set_mutator(&mut self, mutator: &Ref<dyn StateMutator>) {
            #output
        }
    };
}

fn get_new_complete_method(
    state_name: &Ident,
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
        pub fn state_new_complete(#args) -> #state_name {
            #state_name {
                #fields
            }
        }
    };
}

fn get_read_to_type_method(
    type_name: &Ident,
    state_name: &Ident,
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
        fn state_read_to_type(reader: &mut PacketReader) -> #type_name {
            #prop_reads

            return #type_name::#state_name(Ref::new(#state_name {
                #prop_names
            }));
        }
    };
}

fn get_get_typed_copy_method(
    type_name: &Ident,
    state_name: &Ident,
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
        fn state_get_typed_copy(&self) -> #type_name {
            let copied_state = #state_name::state_new_complete(#args).state_wrap();
            return #type_name::#state_name(copied_state);
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
        fn state_write_partial(&self, diff_mask: &DiffMask, buffer: &mut Vec<u8>) {

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
        fn state_read_full(&mut self, reader: &mut PacketReader, packet_index: u16) {
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
        fn state_read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16) {
            #output
        }
    };
}

fn get_equals_method(state_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn state_equals(&self, other: &#state_name) -> bool {
            #output
            return true;
        }
    };
}

fn get_mirror_method(state_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
        fn state_mirror(&mut self, other: &#state_name) {
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

fn get_type_name(input: &DeriveInput, type_type: &str) -> Ident {
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

    return type_name_option.expect(
        format!(
            "#[derive({})] requires an accompanying #[type_name = \"{} Type Name Here\"] attribute",
            type_type, type_type
        )
        .as_str(),
    );
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
        fn state_write(&self, buffer: &mut Vec<u8>) {
            #output
        }
    };
}