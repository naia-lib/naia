use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn entity_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let read_full_method = get_read_full_method(&type_name, &input.data);
    let read_partial_method = get_read_partial_method(&type_name, &input.data);
    let inner_ref_method = get_inner_ref_method(&type_name, &input.data);
    let conversion_methods = get_conversion_methods(&type_name, &input.data);
    let equals_method = get_equals_method(&type_name, &input.data);
    let set_to_interpolation_method = get_set_to_interpolation_method(&type_name, &input.data);
    let interpolate_with_method = get_interpolate_with_method(&type_name, &input.data);
    let is_interpolated_method = get_is_interpolated_method(&type_name, &input.data);
    let mirror_method = get_mirror_method(&type_name, &input.data);

    let gen = quote! {
        use naia_shared::{EntityType, Entity, EntityEq, StateMask};
        impl EntityType for #type_name {
            #read_full_method
            #read_partial_method
            #inner_ref_method
            #equals_method
            #set_to_interpolation_method
            #interpolate_with_method
            #is_interpolated_method
            #mirror_method
        }
        #conversion_methods
    };

    proc_macro::TokenStream::from(gen)
}

fn get_read_full_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        identity.as_ref().borrow_mut().read_full(bytes, packet_index);
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn read_full(&mut self, bytes: &[u8], packet_index: u16) {
            match self {
                #variants
            }
        }
    };
}

fn get_read_partial_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        identity.as_ref().borrow_mut().read_partial(state_mask, bytes, packet_index);
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8], packet_index: u16) {
            match self {
                #variants
            }
        }
    };
}

fn get_inner_ref_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let method_name = Ident::new(
                    (variant_name.to_string() + "Convert").as_str(),
                    Span::call_site(),
                );

                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        return #method_name(identity.clone());
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn inner_ref(&self) -> Rc<RefCell<dyn Entity<#type_name>>> {
            match self {
                #variants
            }
        }
    };
}

fn get_conversion_methods(type_name: &Ident, data: &Data) -> TokenStream {
    return match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let method_name = Ident::new(
                    (variant_name.to_string() + "Convert").as_str(),
                    Span::call_site(),
                );

                let new_output_right = quote! {
                    fn #method_name(eref: Rc<RefCell<#variant_name>>) -> Rc<RefCell<dyn Entity<#type_name>>> {
                        eref.clone()
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };
}

fn get_equals_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        match other {
                            #type_name::#variant_name(other_identity) => {
                                return identity.as_ref().borrow().equals(&other_identity.as_ref().borrow());
                            }
                            _ => { return false; }
                        }
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn equals(&self, other: &#type_name) -> bool {
            match self {
                #variants
            }
        }
    };
}

fn get_set_to_interpolation_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        match old {
                            #type_name::#variant_name(old_identity) => {
                                match new {
                                    #type_name::#variant_name(new_identity) => {
                                        return identity.borrow_mut().set_to_interpolation(&old_identity.as_ref().borrow(), &new_identity.as_ref().borrow(), fraction);
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn set_to_interpolation(&mut self, old: &#type_name, new: &#type_name, fraction: f32) {
            match self {
                #variants
            }
        }
    };
}

fn get_interpolate_with_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        match other {
                            #type_name::#variant_name(other_identity) => {
                                        return identity.borrow_mut().interpolate_with(&other_identity.as_ref().borrow(), fraction);
                                    }
                            _ => {}
                        }
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn interpolate_with(&mut self, other: &#type_name, fraction: f32) {
            match self {
                #variants
            }
        }
    };
}

fn get_mirror_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        match other {
                            #type_name::#variant_name(other_identity) => {
                                        return identity.borrow_mut().mirror(&other_identity.as_ref().borrow());
                                    }
                            _ => {}
                        }
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn mirror(&mut self, other: &#type_name) {
            match self {
                #variants
            }
        }
    };
}

fn get_is_interpolated_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(identity) => {
                        return identity.borrow().is_interpolated();
                    }
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
            output
        }
        _ => unimplemented!(),
    };

    return quote! {
        fn is_interpolated(&self) -> bool {
            match self {
                #variants
            }
        }
    };
}

////FROM THIS
//#[derive(EntityType)]
//pub enum ExampleEntity {
//    PointEntity(Rc<RefCell<PointEntity>>),
//}

////TO THIS
//impl EntityType for ExampleEntity {
//    fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8],
// packet_index: u16) {        match self {
//            ExampleEntity::PointEntity(identity) => {
//                identity.as_ref().borrow_mut().read_partial(state_mask,
// bytes, packet_index);            }
//        }
//    }
//}
