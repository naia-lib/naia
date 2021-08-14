use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident, Type};

pub fn event_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let event_name = &input.ident;
    let event_builder_name = Ident::new(
        (event_name.to_string() + "Builder").as_str(),
        Span::call_site(),
    );

    let properties = get_properties(&input);

    let type_name = get_type_name(&input, "Event");

    let event_write_method = get_write_method(&properties);

    let new_complete_method = get_new_complete_method(event_name, &properties);

    let read_to_type_method = get_read_to_type_method(&type_name, event_name, &properties);

    let gen = quote! {
        use std::{any::TypeId, io::Cursor};
        use naia_shared::{EventBuilder, PacketReader};
        pub struct #event_builder_name {
            type_id: TypeId,
        }
        impl EventBuilder<#type_name> for #event_builder_name {
            fn event_get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn event_build(&self, reader: &mut PacketReader) -> #type_name {
                return #event_name::event_read_to_type(reader);
            }
        }
        impl #event_name {
            pub fn event_get_builder() -> Box<dyn EventBuilder<#type_name>> {
                return Box::new(#event_builder_name {
                    type_id: TypeId::of::<#event_name>(),
                });
            }
            #new_complete_method
            #read_to_type_method
        }
        impl State<#type_name> for #event_name {
            fn event_is_guaranteed(&self) -> bool {
                #event_name::is_guaranteed()
            }
            #event_write_method
            fn event_get_typed_copy(&self) -> #type_name {
                return #type_name::#event_name(self.clone());
            }
            fn event_get_type_id(&self) -> TypeId {
                return TypeId::of::<#event_name>();
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_new_complete_method(event_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {
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
            #field_name: Property::<#field_type>::new(#field_name, 0),
        };
        let new_output_result = quote! {
            #fields
            #new_output_right
        };
        fields = new_output_result;
    }

    return quote! {
        pub fn event_new_complete(#args) -> #event_name {
            #event_name {
                #fields
            }
        }
    };
}

fn get_read_to_type_method(
    type_name: &Ident,
    event_name: &Ident,
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
            let mut #field_name = Property::<#field_type>::new(Default::default(), 0);
            #field_name.read(reader, 1);
        };
        let new_output_result = quote! {
            #prop_reads
            #new_output_right
        };
        prop_reads = new_output_result;
    }

    return quote! {
        fn event_read_to_type(reader: &mut PacketReader) -> #type_name {
            #prop_reads

            return #type_name::#event_name(#event_name {
                #prop_names
            });
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
        fn event_write(&self, buffer: &mut Vec<u8>) {
            #output
        }
    };
}