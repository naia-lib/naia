use proc_macro2::{TokenStream, Span};
use quote::{quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident, Meta, Lit, MetaNameValue, Fields, Field};

pub fn event_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {

    let input = parse_macro_input!(input as DeriveInput);

    let event_name = &input.ident;
    let event_builder_name = Ident::new((event_name.to_string() + "Builder").as_str(), Span::call_site());

    let mut type_name: Option<Ident> = None;

    let properties = get_properties(&input);

    for option in input.attrs.into_iter() {
        let option = option.parse_meta().unwrap();
        match option {
            Meta::NameValue(meta_name_value) => {
                let path = meta_name_value.path;
                let lit = meta_name_value.lit;
                if let Some(ident) = path.get_ident() {
                    if ident == "type_name" {
                        if let Lit::Str(lit) = lit {
                            let ident = Ident::new(lit.value().as_str(), Span::call_site());
                            type_name = Some(ident);
                        }
                    }
                }
            },
            _ => {}
        }
    }

    if type_name.is_none() {
        panic!("#[derive(Event)] requires an accompanying #[type_name = \"{Event Type Name Here}\"] attribute");
    }

    let event_write_method = get_event_write_method(&properties);

    let gen = quote! {
        pub struct #event_builder_name {
            type_id: TypeId,
        }
        impl EventBuilder<#type_name> for #event_builder_name {
            fn get_type_id(&self) -> TypeId {
                return self.type_id;
            }
            fn build(&self, buffer: &[u8]) -> #type_name {
                return #event_name::read_to_type(buffer);
            }
        }
        impl #event_name {
            pub fn get_builder() -> Box<dyn EventBuilder<#type_name>> {
                return Box::new(#event_builder_name {
                    type_id: TypeId::of::<#event_name>(),
                });
            }
        }
        impl Event<#type_name> for #event_name {
            fn is_guaranteed(&self) -> bool {
                #event_name::is_guaranteed()
            }
            #event_write_method
            fn get_typed_copy(&self) -> ExampleEvent {
                return ExampleEvent::StringEvent(self.clone());
            }
            fn get_type_id(&self) -> TypeId {
                return TypeId::of::<StringEvent>();
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_properties(input: &DeriveInput) -> Vec<Field> {
    let mut fields = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in fields_named.named.iter() {
                fields.push(field.clone());
            }
        }
    }

    fields
}

fn get_event_write_method(properties: &Vec<Field>) -> TokenStream {

    /* quote! {
        fn write(&self, buffer: &mut Vec<u8>) {
            PropertyIo::write(&self.message, buffer);
        }
    }; */

    let mut output = quote! {};

    for field in properties.iter() {
        if let Some(property_name) = &field.ident {
            let new_output_right = quote! {
                PropertyIo::write(&self.#property_name, buffer);
            };
            let new_output_result = quote! {
                #output
                #new_output_right
            };
            output = new_output_result;
        }
    }

    return quote! {
        fn write(&self, buffer: &mut Vec<u8>) {
            #output
        }
    }
}

////FROM THIS
//#[derive(Event, Clone)]
//#[type_name = "ExampleType"]
//pub struct StringEvent {
//    pub message: Property<String>,
//}

////TO THIS
//pub struct StringEventBuilder {
//    type_id: TypeId,
//}
//
//impl EventBuilder<ExampleEvent> for StringEventBuilder {
//    fn get_type_id(&self) -> TypeId {
//        return self.type_id;
//    }
//
//    fn build(&self, buffer: &[u8]) -> ExampleEvent {
//        return StringEvent::read_to_type(buffer);
//    }
//}
//
//impl StringEvent {
//    pub fn get_builder() -> Box<dyn EventBuilder<ExampleEvent>> {
//        return Box::new(StringEventBuilder {
//            type_id: TypeId::of::<StringEvent>(),
//        });
//    }

//    pub fn new_complete(message: String) -> StringEvent {
//        StringEvent {
//            message: Property::<String>::new(message, 0),
//        }
//    }
//
//    fn read_to_type(buffer: &[u8]) -> ExampleEvent {
//        let read_cursor = &mut Cursor::new(buffer);
//        let mut message = Property::<String>::new(Default::default(), 0);
//        message.read(read_cursor);
//
//        return ExampleEvent::StringEvent(StringEvent {
//            message,
//        });
//    }
//}
//impl Event<ExampleEvent> for StringEvent {
//    fn is_guaranteed(&self) -> bool {
//        StringEvent::is_guaranteed()
//    }
//    fn write(&self, buffer: &mut Vec<u8>) {
//        PropertyIo::write(&self.message, buffer);
//    }
//    fn get_typed_copy(&self) -> ExampleEvent {
//        return ExampleEvent::StringEvent(self.clone());
//    }
//    fn get_type_id(&self) -> TypeId {
//        return TypeId::of::<StringEvent>();
//    }
//}