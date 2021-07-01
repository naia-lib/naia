use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn actor_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let read_full_method = get_read_full_method(&type_name, &input.data);
    let read_partial_method = get_read_partial_method(&type_name, &input.data);
    let inner_ref_method = get_inner_ref_method(&type_name, &input.data);
    let conversion_methods = get_conversion_methods(&type_name, &input.data);
    let equals_method = get_equals_method(&type_name, &input.data);
    let equals_prediction_method = get_equals_prediction_method(&type_name, &input.data);
    let mirror_method = get_mirror_method(&type_name, &input.data);
    let is_predicted_method = get_is_predicted_method(&type_name, &input.data);

    let gen = quote! {
        use naia_shared::{ActorType, Actor, ActorEq, StateMask, PacketReader};
        impl ActorType for #type_name {
            #read_full_method
            #read_partial_method
            #inner_ref_method
            #equals_method
            #equals_prediction_method
            #is_predicted_method
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
                    #type_name::#variant_name(idactor) => {
                        idactor.borrow_mut().read_full(reader, packet_index);
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
        fn read_full(&mut self, reader: &mut PacketReader, packet_index: u16) {
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
                    #type_name::#variant_name(idactor) => {
                        idactor.borrow_mut().read_partial(state_mask, reader, packet_index);
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
        fn read_partial(&mut self, state_mask: &StateMask, reader: &mut PacketReader, packet_index: u16) {
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
                    #type_name::#variant_name(idactor) => {
                        return #method_name(idactor.clone());
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
        fn inner_ref(&self) -> Ref<dyn Actor<#type_name>> {
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

                let method_name_raw = Ident::new(
                    (variant_name.to_string() + "ConvertRaw").as_str(),
                    Span::call_site(),
                );

                cfg_if! {
                    if #[cfg(feature = "multithread")] {
                        let multithread = true;
                    } else {
                        let multithread = false;
                    }
                }

                {
                    let new_output_right = {
                        if multithread {
                            quote! {
                                use std::{sync::{Arc, Mutex}};
                                fn #method_name_raw(eref: Arc<Mutex<#variant_name>>) -> Arc<Mutex<dyn Actor<#type_name>>> {
                                    eref.clone()
                                }
                            }
                        } else {
                            quote! {
                                use std::{rc::Rc, cell::RefCell};
                                fn #method_name_raw(eref: Rc<RefCell<#variant_name>>) -> Rc<RefCell<dyn Actor<#type_name>>> {
                                    eref.clone()
                                }
                            }
                        }
                    };

                    let new_output_result = quote! {
                        #output
                        #new_output_right
                    };

                    output = new_output_result;
                }

                {
                    let new_output_right = quote! {
                        fn #method_name(eref: Ref<#variant_name>) -> Ref<dyn Actor<#type_name>> {
                            let upcast_ref = #method_name_raw(eref.inner());
                            Ref::new_raw(upcast_ref)
                        }
                    };
                    let new_output_result = quote! {
                        #output
                        #new_output_right
                    };
                    output = new_output_result;
                }
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
                    #type_name::#variant_name(idactor) => {
                        match other {
                            #type_name::#variant_name(other_idactor) => {
                                return idactor.borrow().equals(&other_idactor.borrow());
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

fn get_equals_prediction_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idactor) => {
                        match other {
                            #type_name::#variant_name(other_idactor) => {
                                return idactor.borrow().equals_prediction(&other_idactor.borrow());
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
        fn equals_prediction(&self, other: &#type_name) -> bool {
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
                    #type_name::#variant_name(idactor) => {
                        match other {
                            #type_name::#variant_name(other_idactor) => {
                                        return idactor.borrow_mut().mirror(&other_idactor.borrow());
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

fn get_is_predicted_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idactor) => {
                        return idactor.borrow().is_predicted();
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
        fn is_predicted(&self) -> bool {
            match self {
                #variants
            }
        }
    };
}
