use proc_macro2::{TokenStream, Span};
use quote::{quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident, Meta, Lit, MetaNameValue};

pub fn event_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {

    let input = parse_macro_input!(input as DeriveInput);

    let event_name = input.ident;
    let event_builder_name = Ident::new((event_name.to_string() + "Builder").as_str(), Span::call_site());

    let mut type_name: Option<Ident> = None;

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
            fn write(&self, buffer: &mut Vec<u8>) {
                PropertyIo::write(&self.message, buffer);
            }
            fn get_typed_copy(&self) -> ExampleEvent {
                return ExampleEvent::StringEvent(self.clone());
            }
            fn get_type_id(&self) -> TypeId {
                return TypeId::of::<StringEvent>();
            }
        }
    };

    //TODO: write() method directly above (in impl Event<#type_name> fpr #event_name block) needs to be implemented!

    proc_macro::TokenStream::from(gen)
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
//
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