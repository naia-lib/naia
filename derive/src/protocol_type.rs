use proc_macro2::{Span, TokenStream, Punct, Spacing};
use quote::{format_ident, quote};
use syn::{Data, Ident};

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
    let mut variant_index: u16 = 0;

    {

        for variant in properties {
            let variant_name = Ident::new(
                &variant.to_string(),
                Span::call_site(),
            );

            let new_output_right = quote! {
                #variant_name = #variant_index,
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
        let mut variant_match_index: u16 = 0;

        for variant in properties {
            let variant_name = Ident::new(
                &variant.to_string(),
                Span::call_site(),
            );

            let new_output_right = quote! {
                #variant_match_index => #enum_name::#variant_name,
            };
            let new_output_result = quote! {
                #variant_match
                #new_output_right
            };
            variant_match = new_output_result;

            variant_match_index += 1;
        }
    }

    return quote! {
        #hashtag[repr(u16)]
        #hashtag[derive(Hash, Eq, PartialEq, Copy, Clone)]
        pub enum #enum_name {
            #variant_list
            UNKNOWN = #variant_index,
        }

        impl ProtocolKindType for #enum_name {
            fn to_u16(&self) -> u16 {
                return *self as u16;
            }
            fn from_u16(val: u16) -> Self {
                match val {
                    #variant_match
                    _ => #enum_name::UNKNOWN,
                }
            }
        }
    };
}

pub fn get_kind_of_method(enum_name: &Ident, properties: &Vec<Ident>) -> TokenStream {

    let mut const_list = quote! {};

    {
        for variant in properties {
            let variant_name_string = variant.to_string();
            let variant_name_ident = Ident::new(
                &variant_name_string,
                Span::call_site(),
            );

            let id_const_name = format_ident!("{}_TYPE_ID", variant_name_string.to_uppercase().as_str());
            let id_const_ident = Ident::new(
                &id_const_name.to_string(),
                Span::call_site(),
            );
            let new_output_right = quote! {
                const #id_const_ident: TypeId = TypeId::of::<#variant_name_ident>();
            };
            let new_output_result = quote! {
                #const_list
                #new_output_right
            };
            const_list = new_output_result;
        }
    }

    let mut match_branches = quote! {};

    {
        for variant in properties {
            let variant_name_string = variant.to_string();
            let variant_name_ident = Ident::new(
                &variant_name_string,
                Span::call_site(),
            );

            let id_const_name = format_ident!("{}_TYPE_ID", variant_name_string.to_uppercase().as_str());
            let id_const_ident = Ident::new(
                &id_const_name.to_string(),
                Span::call_site(),
            );
            let new_output_right = quote! {
                #id_const_ident => #enum_name::#variant_name_ident,
            };
            let new_output_result = quote! {
                #match_branches
                #new_output_right
            };
            match_branches = new_output_result;
        }
    }

    return quote! {
        fn kind_of<R: Replicate<Self>>() -> Self::Kind {
            #const_list
            match TypeId::of::<R>() {
                #match_branches
                _ => #enum_name::UNKNOWN,
            }
        }
    };
}

pub fn get_dyn_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn dyn_ref(&self) -> DynRef<'_, Self> {
            match self {
                #variants
            }
        }
    };
}

pub fn get_dyn_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn dyn_mut(&mut self) -> DynMut<'_, Self> {
            match self {
                #variants
            }
        }
    };
}

pub fn get_cast_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn cast_ref<R: Replicate<Self>>(&self) -> Option<&R> {
            match self {
                #variants
            }
        }
    };
}

pub fn get_cast_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn cast_mut<R: Replicate<Self>>(&mut self) -> Option<&mut R> {
            match self {
                #variants
            }
        }
    };
}

pub fn get_cast_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(replica) => {
                        if let Some(any_ref) = Any::downcast_ref::<R>(&replica) {
                            return Some(R::copy(any_ref));
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
        fn cast<R: ReplicateEq<Self>>(self) -> Option<R> {
            match self {
                #variants
            }

            return None;
        }
    };
}

pub fn get_load_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
