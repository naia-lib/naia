use proc_macro2::{TokenStream, Span, Punct, Spacing};
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Ident, Type};

use super::utils;

pub fn entity_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {

    let input = parse_macro_input!(input as DeriveInput);

    let entity_name = &input.ident;
    let entity_builder_name = Ident::new((entity_name.to_string() + "Builder").as_str(), Span::call_site());

    let type_name = utils::get_type_name(&input, "Entity");

    let properties = utils::get_properties(&input);

    //let read_to_type_method = get_read_to_type_method(&type_name, entity_name, &properties);

    let property_enum = get_property_enum(entity_name, &properties);

    let gen = quote! {
        #property_enum
    };

    proc_macro::TokenStream::from(gen)
}

fn get_property_enum(entity_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {

    let hashtag = Punct::new('#', Spacing::Alone);

    let enum_name = format_ident!("{}Prop", entity_name);

    let mut variant_index = 0;
    let mut variant_list = quote! {};
    for (variant, _) in properties {

        let mut uppercase_variant_name = variant.to_string();
        uppercase_variant_name = uppercase_variant_name.to_uppercase();

        let new_output_right = quote! {
            #uppercase_variant_name = #variant_index
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
            X = 0,
            Y = 1,
        }
    };
}

fn get_read_to_type_method(type_name: &Ident, event_name: &Ident, properties: &Vec<(Ident, Type)>) -> TokenStream {

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

            return #type_name::#event_name(#event_name {
                #prop_names
            });
        }
    }
}

//FROM THIS:
//#[derive(Entity)]
//#[type_name = "ExampleEntity"]
//pub struct PointEntity {
//    pub x: Property<u8>,
//    pub y: Property<u8>,
//}

//TO THIS:
//#[repr(u8)]
//enum PointEntityProp {
//    X = 0,
//    Y = 1,
//}
//pub struct PointEntityBuilder {
//    type_id: TypeId,
//}
//impl EntityBuilder<ExampleEntity> for PointEntityBuilder {
//    fn build(&self, buffer: &[u8]) -> ExampleEntity {
//        return PointEntity::read_to_type(buffer);
//    }
//    fn get_type_id(&self) -> TypeId {
//        return self.type_id;
//    }
//}
//impl PointEntity {
//    pub fn get_builder() -> Box<dyn EntityBuilder<ExampleEntity>> {
//        return Box::new(PointEntityBuilder {
//            type_id: TypeId::of::<PointEntity>(),
//        });
//    }
//    pub fn wrap(self) -> Rc<RefCell<PointEntity>> {
//        return Rc::new(RefCell::new(self));
//    }
//    pub fn new_complete(x: u8, y: u8) -> PointEntity {
//        PointEntity {
//            x: Property::<u8>::new(x, PointEntityProp::X as u8),
//            y: Property::<u8>::new(y, PointEntityProp::Y as u8),
//        }
//    }
//    fn read_to_type(buffer: &[u8]) -> ExampleEntity {
//        let read_cursor = &mut Cursor::new(buffer);
//        let mut x = Property::<u8>::new(Default::default(), PointEntityProp::X as u8);
//        x.read(read_cursor);
//        let mut y = Property::<u8>::new(Default::default(), PointEntityProp::Y as u8);
//        y.read(read_cursor);
//
//        return ExampleEntity::PointEntity(Rc::new(RefCell::new(PointEntity {
//            x,
//            y,
//        })));
//    }
//}
//impl Entity<ExampleEntity> for PointEntity {
//    fn get_state_mask_size(&self) -> u8 {
//        1
//    }
//    fn get_typed_copy(&self) -> ExampleEntity {
//        let copied_entity = PointEntity::new_complete(*self.x.get(), *self.y.get()).wrap();
//        return ExampleEntity::PointEntity(copied_entity);
//    }
//    fn get_type_id(&self) -> TypeId {
//        return TypeId::of::<PointEntity>();
//    }
//    fn write(&self, buffer: &mut Vec<u8>) {
//        PropertyIo::write(&self.x, buffer);
//        PropertyIo::write(&self.y, buffer);
//    }
//    fn write_partial(&self, state_mask: &StateMask, buffer: &mut Vec<u8>) {
//        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
//            PropertyIo::write(&self.x, buffer);
//        }
//        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
//            PropertyIo::write(&self.y, buffer);
//        }
//    }
//    fn read_partial(&mut self, state_mask: &StateMask, buffer: &[u8]) {
//        let read_cursor = &mut Cursor::new(buffer);
//        if let Some(true) = state_mask.get_bit(PointEntityProp::X as u8) {
//            PropertyIo::read(&mut self.x, read_cursor);
//        }
//        if let Some(true) = state_mask.get_bit(PointEntityProp::Y as u8) {
//            PropertyIo::read(&mut self.y, read_cursor);
//        }
//    }
//    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>) {
//        self.x.set_mutator(mutator);
//        self.y.set_mutator(mutator);
//    }
//}