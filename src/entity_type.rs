use proc_macro2::TokenStream;
use quote::{quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident};
use syn::buffer::TokenBuffer;

pub fn entity_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {

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
    match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {

            };
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            };
            output
        }
        _ => unimplemented!()
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