use proc_macro2::{Span, TokenStream, Punct, Spacing};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn protocol_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let protocol_name = input.ident;
    
    let variants = get_variants(&input.data);

    let kind_enum_name = format_ident!("{}Kind", protocol_name);
    let kind_enum_def = get_kind_enum(&kind_enum_name, &variants);

    let load_method = get_load_method(&protocol_name, &input.data);
    let dyn_ref_method = get_dyn_ref_method(&protocol_name, &input.data);
    let dyn_mut_method = get_dyn_mut_method(&protocol_name, &input.data);
    let cast_ref_method = get_cast_ref_method(&protocol_name, &input.data);
    let cast_mut_method = get_cast_mut_method(&protocol_name, &input.data);

    let gen = quote! {
        use std::any::{Any, TypeId};
        use naia_shared::{ProtocolType, ProtocolKindType, Replicate, ReplicateEq, DiffMask, PacketReader, EntityType, DynRef, DynMut};

        #kind_enum_def

        impl #protocol_name {
            #load_method
        }

        impl ProtocolType for #protocol_name {
            type Kind = #kind_enum_name;
            #dyn_ref_method
            #dyn_mut_method
            #cast_ref_method
            #cast_mut_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

pub fn get_variants(data: &Data) -> Vec<Ident> {
    let mut variants = Vec::new();
    if let Data::Enum(ref data) = *data {
        for variant in data.variants.iter() {
            variants.push(variant.ident.clone());
        }
    }
    return variants;
}

pub fn get_kind_enum(enum_name: &Ident, properties: &Vec<Ident>) -> TokenStream {
    let hashtag = Punct::new('#', Spacing::Alone);

    let mut variant_list = quote! {};

    {
        let mut variant_index: u16 = 0;
        for variant in properties {
            let uppercase_variant_name = Ident::new(
                &variant.to_string(),
                Span::call_site(),
            );

            let new_output_right = quote! {
            #uppercase_variant_name = #variant_index,
        };
            let new_output_result = quote! {
            #variant_list
            #new_output_right
        };
            variant_list = new_output_result;

            variant_index += 1;
        }
    }

    let mut variant_match = quote! {};
    {
        let mut variant_index: u16 = 0;

        for variant in properties {
            let uppercase_variant_name = Ident::new(
                &variant.to_string(),
                Span::call_site(),
            );

            let new_output_right = quote! {
                #variant_index => #enum_name::#uppercase_variant_name,
            };
            let new_output_result = quote! {
                #variant_match
                #new_output_right
            };
            variant_match = new_output_result;

            variant_index += 1;
        }
    }

    return quote! {
        #hashtag[repr(u16)]
        #hashtag[derive(Hash, Eq, PartialEq, Copy, Clone)]
        pub enum #enum_name {
            #variant_list
        }

        impl ProtocolKindType for #enum_name {
            fn to_u16(&self) -> u16 {
                return *self;
            }
            fn from_u16(val: u16) -> Self {
                match val {
                    #variant_match
                }
            }
        }
    };
}

fn get_dyn_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(inner) => {
                        return DynRef::new(inner);
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
        fn dyn_ref(&self) -> DynRef<'_, Self, Self::Kind> {
            match self {
                #variants
            }
        }
    };
}

fn get_dyn_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(inner) => {
                        return DynMut::new(inner);
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
        fn dyn_mut(&mut self) -> DynMut<'_, Self, Self::Kind> {
            match self {
                #variants
            }
        }
    };
}

fn get_cast_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(replica_ref) => {
                        let typed_ref = replica_ref as &dyn Any;
                        return typed_ref.downcast_ref::<R>();
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
        fn cast_ref<R: Replicate>(&self) -> Option<&R> {
            match self {
                #variants
            }
        }
    };
}

fn get_cast_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(replica_ref) => {
                        let typed_ref = replica_ref as &mut dyn Any;
                        return typed_ref.downcast_mut::<R>();
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
        fn cast_mut<R: Replicate>(&mut self) -> Option<&mut R> {
            match self {
                #variants
            }
        }
    };
}

fn get_load_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        pub fn load() -> Manifest<#protocol_name> {
            let mut manifest = Manifest::<#protocol_name>::new();

            #variants

            manifest
        }
    };
}
