use proc_macro2::TokenStream;
use quote::{quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident};

#[proc_macro_derive(EntityType)]
pub fn entity_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {

    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let variants = get_variants(&type_name, &input.data);

    let gen = quote! {
        impl EntityType for #type_name {
            fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]) {
                match self {
                    #variants
                }
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_variants(type_name: &Ident, data: &Data) -> TokenStream {
    quote ! {
        #type_name::PointEntity(identity) => {
            identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
        }
    }
}


////FROM THIS
//#[derive(EntityType)]
//pub enum ExampleEntity {
//    PointEntity(Rc<RefCell<PointEntity>>),
//}

////TO THIS
//impl EntityType for ExampleEntity {
//
//    //TODO: Candidate for Macro
//    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]) {
//        match self {
//            ExampleEntity::PointEntity(identity) => {
//                identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
//            }
//        }
//    }
//}