use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn protocol_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let type_name = input.ident;

    let load_method = get_load_method(&type_name, &input.data);

    let dyn_ref_method = get_dyn_ref_method(&type_name, &input.data);
    let dyn_mut_method = get_dyn_mut_method(&type_name, &input.data);
    let cast_ref_method = get_cast_ref_method(&type_name, &input.data);
    let cast_mut_method = get_cast_mut_method(&type_name, &input.data);

    let gen = quote! {
        use std::any::{Any, TypeId};
        use naia_shared::{ProtocolType, Replicate, ReplicateEq, DiffMask, PacketReader, EntityType};
        impl #type_name {
            #load_method
        }
        impl ProtocolType for #type_name {
            #dyn_ref_method
            #dyn_mut_method
            #cast_ref_method
            #cast_mut_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn get_dyn_ref_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let convert_method_name = Ident::new(
                    (variant_name.to_string() + "Convert").as_str(),
                    Span::call_site(),
                );

                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        return #type_name::#convert_method_name(replica_ref.clone());
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
        fn dyn_ref(&self) -> &dyn Replicate<#type_name> {
            match self {
                #variants
            }
        }
    };
}

fn get_dyn_mut_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let convert_method_name = Ident::new(
                    (variant_name.to_string() + "Convert").as_str(),
                    Span::call_site(),
                );

                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        return #type_name::#convert_method_name(replica_ref.clone());
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
        fn dyn_mut(&mut self) -> &mut dyn Replicate<#type_name> {
            match self {
                #variants
            }
        }
    };
}

fn get_cast_ref_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        let typed_ref = replica_ref as &dyn Any;
                        return typed_ref.downcast_ref::<Ref<R>>();
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
        fn cast_ref<R: Replicate<#type_name>>(&self) -> Option<&R> {
            match self {
                #variants
            }
        }
    };
}

fn get_cast_mut_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        let typed_ref = replica_ref as &dyn Any;
                        return typed_ref.downcast_ref::<Ref<R>>();
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
        fn cast_mut<R: Replicate<#type_name>>(&mut self) -> Option<&mut R> {
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
