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

pub fn protocol_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let read_full_method = get_read_full_method(&type_name, &input.data);
    let read_partial_method = get_read_partial_method(&type_name, &input.data);
    let inner_ref_method = get_inner_ref_method(&type_name, &input.data);
    let conversion_methods = get_conversion_methods(&type_name, &input.data);
    let equals_method = get_equals_method(&type_name, &input.data);
    let mirror_method = get_mirror_method(&type_name, &input.data);
    let write_method = get_write_method(&type_name, &input.data);
    let get_type_id_method = get_type_id_method(&type_name, &input.data);
    let load_method = get_load_method(&type_name, &input.data);
    let copy_method = get_copy_method(&type_name, &input.data);

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
        use naia_shared::{ProtocolType, Replicate, ReplicaEq, DiffMask, PacketReader};
        #ref_imports
        impl #type_name {
            #load_method
        }
        impl ProtocolType for #type_name {
            #read_full_method
            #read_partial_method
            #inner_ref_method
            #equals_method
            #mirror_method
            #write_method
            #get_type_id_method
            #copy_method
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
                    #type_name::#variant_name(replica_ref) => {
                        replica_ref.borrow_mut().read_full(reader, packet_index);
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
                    #type_name::#variant_name(replica_ref) => {
                        replica_ref.borrow_mut().read_partial(diff_mask, reader, packet_index);
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
                    #type_name::#variant_name(replica_ref) => {
                        return #method_name(replica_ref.clone());
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
        fn inner_ref(&self) -> Ref<dyn Replicate<#type_name>> {
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
                                fn #method_name_raw(eref: Arc<Mutex<#variant_name>>) -> Arc<Mutex<dyn Replicate<#type_name>>> {
                                    eref.clone()
                                }
                            }
                        } else {
                            quote! {
                                fn #method_name_raw(eref: Rc<RefCell<#variant_name>>) -> Rc<RefCell<dyn Replicate<#type_name>>> {
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
                        fn #method_name(eref: Ref<#variant_name>) -> Ref<dyn Replicate<#type_name>> {
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
                    #type_name::#variant_name(replica_ref) => {
                        match other {
                            #type_name::#variant_name(other_ref) => {
                                return replica_ref.borrow().equals(&other_ref.borrow());
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
                    #type_name::#variant_name(replica_ref) => {
                        match other {
                            #type_name::#variant_name(other_ref) => {
                                        return replica_ref.borrow_mut().mirror(&other_ref.borrow());
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

fn get_write_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        replica_ref.borrow().write(buffer);
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
        fn write(&self, buffer: &mut Vec<u8>) {
            match self {
                #variants
            }
        }
    };
}

fn get_type_id_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        return replica_ref.borrow().get_type_id();
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
        fn get_type_id(&self) -> TypeId {
            match self {
                #variants
            }
        }
    };
}

fn get_load_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    manifest.register_replica(#variant_name::get_builder());
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
        pub fn load() -> Manifest<#type_name> {
            let mut manifest = Manifest::<#type_name>::new();

            #variants

            manifest
        }
    };
}

fn get_copy_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        return #type_name::#variant_name(replica_ref.borrow().copy().to_ref());
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
        fn copy(&self) -> #type_name {
            match self {
                #variants
            }
        }
    };
}
