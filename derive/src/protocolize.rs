use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Ident};

pub fn protocolize_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let protocol_name = input.ident;

    let variants = variants(&input.data);

    let kind_enum_name = format_ident!("{}Kind", protocol_name);
    let kind_enum_def = kind_enum(&kind_enum_name, &variants);
    let kind_of_method = kind_of_method();
    let type_to_kind_method = type_to_kind_method(&kind_enum_name, &variants);
    let load_method = load_method(&protocol_name, &input.data);
    let dyn_ref_method = dyn_ref_method(&protocol_name, &input.data);
    let dyn_mut_method = dyn_mut_method(&protocol_name, &input.data);
    let cast_method = cast_method(&protocol_name, &input.data);
    let clone_method = clone_method(&protocol_name, &input.data);
    let cast_ref_method = cast_ref_method(&protocol_name, &input.data);
    let cast_mut_method = cast_mut_method(&protocol_name, &input.data);
    let extract_and_insert_method = extract_and_insert_method(&protocol_name, &input.data);

    let gen = quote! {
        use std::{any::{Any, TypeId}, ops::{Deref, DerefMut}, sync::RwLock, collections::HashMap};
        use naia_shared::{Protocolize, ProtocolInserter, ProtocolKindType, ReplicateSafe,
            DiffMask, PacketReader, ReplicaDynRef, ReplicaDynMut, Replicate, Manifest};

        #kind_enum_def

        impl #protocol_name {
            #load_method
        }

        impl Protocolize for #protocol_name {
            type Kind = #kind_enum_name;
            #kind_of_method
            #type_to_kind_method
            #dyn_ref_method
            #dyn_mut_method
            #cast_method
            #cast_ref_method
            #cast_mut_method
            #extract_and_insert_method
        }

        impl Clone for #protocol_name {
            #clone_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

pub fn variants(data: &Data) -> Vec<Ident> {
    let mut variants = Vec::new();
    if let Data::Enum(ref data) = *data {
        for variant in data.variants.iter() {
            variants.push(variant.ident.clone());
        }
    }
    return variants;
}

pub fn kind_enum(enum_name: &Ident, properties: &Vec<Ident>) -> TokenStream {
    let hashtag = Punct::new('#', Spacing::Alone);

    let mut variant_definitions = quote! {};
    let mut variants_from_u16 = quote! {};
    let mut variants_to_type_id = quote! {};
    let mut variants_size = quote! {};
    let mut variants_size_partial = quote! {};

    let mut variant_index: u16 = 0;

    for variant in properties {
        let variant_name = Ident::new(&variant.to_string(), Span::call_site());

        // Variant definitions
        {
            let new_output_right = quote! {
                #variant_name = #variant_index,
            };
            let new_output_result = quote! {
                #variant_definitions
                #new_output_right
            };
            variant_definitions = new_output_result;
        }

        // Variants from_u16() match branch
        {
            let new_output_right = quote! {
                #variant_index => #enum_name::#variant_name,
            };
            let new_output_result = quote! {
                #variants_from_u16
                #new_output_right
            };
            variants_from_u16 = new_output_result;
        }

        // Variants to_type_id() match branch
        {
            let new_output_right = quote! {
                #enum_name::#variant_name => TypeId::of::<#variant_name>(),
            };
            let new_output_result = quote! {
                #variants_to_type_id
                #new_output_right
            };
            variants_to_type_id = new_output_result;
        }

        // Variants size() match branch
        {
            let new_output_right = quote! {
                #enum_name::#variant_name => #variant_name::size(),
            };
            let new_output_result = quote! {
                #variants_size
                #new_output_right
            };
            variants_size = new_output_result;
        }

        // Variants size_partial() match branch
        {
            let new_output_right = quote! {
                #enum_name::#variant_name => #variant_name::size_partial(diff_mask),
            };
            let new_output_result = quote! {
                #variants_size_partial
                #new_output_right
            };
            variants_size_partial = new_output_result;
        }

        variant_index += 1;
    }

    return quote! {
        #hashtag[repr(u16)]
        #hashtag[derive(Hash, Eq, PartialEq, Copy, Clone)]
        pub enum #enum_name {
            #variant_definitions
            UNKNOWN = #variant_index,
        }

        impl ProtocolKindType for #enum_name {
            fn to_u16(&self) -> u16 {
                return *self as u16;
            }
            fn from_u16(val: u16) -> Self {
                match val {
                    #variants_from_u16
                    _ => #enum_name::UNKNOWN,
                }
            }
            fn to_type_id(&self) -> TypeId {
                match self {
                    #variants_to_type_id
                    _ => TypeId::of::<()>()
                }
            }
            fn size(&self) -> usize {
                match self {
                    #variants_size
                    _ => 0
                }
            }
            fn size_partial(&self, diff_mask: &DiffMask) -> usize {
                match self {
                    #variants_size_partial
                    _ => 0
                }
            }
        }
    };
}

fn kind_of_method() -> TokenStream {
    return quote! {
        fn kind_of<R: ReplicateSafe<Self>>() -> Self::Kind {
            return Self::type_to_kind(TypeId::of::<R>());
        }
    };
}

fn type_to_kind_method(enum_name: &Ident, properties: &Vec<Ident>) -> TokenStream {
    let mut insert_list = quote! {};

    {
        for variant in properties {
            let variant_name_string = variant.to_string();
            let variant_name_ident = Ident::new(&variant_name_string, Span::call_site());

            let new_output_right = quote! {
                map.insert(TypeId::of::<#variant_name_ident>(), #enum_name::#variant_name_ident);
            };
            let new_output_result = quote! {
                #insert_list
                #new_output_right
            };
            insert_list = new_output_result;
        }
    }

    return quote! {
        fn type_to_kind(type_id: TypeId) -> Self::Kind {
            unsafe {
                static mut TYPE_TO_KIND_MAP: Option<RwLock<HashMap<TypeId, #enum_name>>> = None;

                if TYPE_TO_KIND_MAP.is_none() {
                    let mut map: HashMap<TypeId, #enum_name> = HashMap::new();
                    #insert_list
                    TYPE_TO_KIND_MAP = Some((RwLock::new(map)));
                }

                match TYPE_TO_KIND_MAP.as_ref().unwrap().read().unwrap().deref().get(&type_id) {
                    Some(kind) => {
                        return *kind;
                    }
                    None => {
                        return #enum_name::UNKNOWN;
                    }
                }
            }
        }
    };
}

pub fn dyn_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(inner) => {
                        return ReplicaDynRef::new(inner);
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
        fn dyn_ref(&self) -> ReplicaDynRef<'_, Self> {
            match self {
                #variants
            }
        }
    };
}

pub fn dyn_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(inner) => {
                        return ReplicaDynMut::new(inner);
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
        fn dyn_mut(&mut self) -> ReplicaDynMut<'_, Self> {
            match self {
                #variants
            }
        }
    };
}

pub fn cast_ref_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn cast_ref<R: ReplicateSafe<Self>>(&self) -> Option<&R> {
            match self {
                #variants
            }
        }
    };
}

pub fn cast_mut_method(protocol_name: &Ident, data: &Data) -> TokenStream {
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
        fn cast_mut<R: ReplicateSafe<Self>>(&mut self) -> Option<&mut R> {
            match self {
                #variants
            }
        }
    };
}

pub fn cast_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(replica) => {
                        let any_replica: &dyn Any = &replica;
                        if let Some(any_ref) = any_replica.downcast_ref::<R>() {
                            return Some(any_ref.clone());
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
        fn cast<R: Replicate<Self>>(self) -> Option<R> {
            match self {
                #variants
            }

            return None;
        }
    };
}

pub fn clone_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;

                let new_output_right = quote! {
                    #protocol_name::#variant_name(replica) => {
                        return #protocol_name::#variant_name(replica.clone());
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
        fn clone(&self) -> Self {
            match self {
                #variants
            }
        }
    };
}

pub fn load_method(protocol_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    manifest.register_replica(#variant_name::builder());
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

fn extract_and_insert_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        inserter.insert(key, replica_ref.clone());
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
        fn extract_and_insert<E, I: ProtocolInserter<#type_name, E>>(&self,
                                      key: &E,
                                      inserter: &mut I) {
            match self {
                #variants
            }
        }
    };
}
