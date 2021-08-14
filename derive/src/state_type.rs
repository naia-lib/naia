use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Ident};

cfg_if! {
    if #[cfg(feature = "multithread")] {
        const MULTITHREAD: bool = true;
    } else {
        const MULTITHREAD: bool = false;
    }
}

pub fn state_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let read_full_method = get_read_full_method(&type_name, &input.data);
    let read_partial_method = get_read_partial_method(&type_name, &input.data);
    let inner_ref_method = get_inner_ref_method(&type_name, &input.data);
    let conversion_methods = get_conversion_methods(&type_name, &input.data);
    let equals_method = get_equals_method(&type_name, &input.data);
    let mirror_method = get_mirror_method(&type_name, &input.data);
    let write_variants = get_write_variants(&type_name, &input.data);
    let get_type_id_variants = get_type_id_variants(&type_name, &input.data);

    let ref_imports = {
        if MULTITHREAD {
            quote! {
                use std::{sync::{Arc, Mutex}};
            }
        } else {
            quote! {
                use std::{rc::Rc, cell::RefCell};
            }
        }
    };

    let gen = quote! {
        use std::any::TypeId;
        use naia_shared::{StateType, State, StateEq, DiffMask, PacketReader};
        #ref_imports
        impl StateType for #type_name {
            #read_full_method
            #read_partial_method
            #inner_ref_method
            #equals_method
            #mirror_method
            fn write(&self, buffer: &mut Vec<u8>) {
                match self {
                    #write_variants
                }
            }
            fn get_type_id(&self) -> TypeId {
                match self {
                    #get_type_id_variants
                }
            }
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
                    #type_name::#variant_name(idstate) => {
                        idstate.borrow_mut().read_full(reader, packet_index);
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
                    #type_name::#variant_name(idstate) => {
                        idstate.borrow_mut().read_partial(diff_mask, reader, packet_index);
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
        fn read_partial(&mut self, diff_mask: &DiffMask, reader: &mut PacketReader, packet_index: u16) {
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
                    #type_name::#variant_name(idstate) => {
                        return #method_name(idstate.clone());
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
        fn inner_ref(&self) -> Ref<dyn State<#type_name>> {
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

                {
                    let new_output_right = {
                        if MULTITHREAD {
                            quote! {
                                fn #method_name_raw(eref: Arc<Mutex<#variant_name>>) -> Arc<Mutex<dyn State<#type_name>>> {
                                    eref.clone()
                                }
                            }
                        } else {
                            quote! {
                                fn #method_name_raw(eref: Rc<RefCell<#variant_name>>) -> Rc<RefCell<dyn State<#type_name>>> {
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
                        fn #method_name(eref: Ref<#variant_name>) -> Ref<dyn State<#type_name>> {
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
                    #type_name::#variant_name(idstate) => {
                        match other {
                            #type_name::#variant_name(other_idstate) => {
                                return idstate.borrow().equals(&other_idstate.borrow());
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

fn get_mirror_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idstate) => {
                        match other {
                            #type_name::#variant_name(other_idstate) => {
                                        return idstate.borrow_mut().mirror(&other_idstate.borrow());
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

fn get_write_variants(type_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idstate) => {
                        idstate.borrow().write(buffer);
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
    }
}

fn get_type_id_variants(type_name: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(idstate) => {
                        return idstate.borrow().get_type_id();
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
    }
}