use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, Ident};

pub fn get_trait_impl_methods(protocol_name: &Ident, data: &Data) -> TokenStream {
    let mirror_method = get_mirror_method(protocol_name, data);
    let extract_and_insert_method = get_extract_and_insert_method(protocol_name, data);

    return quote! {
        #mirror_method
        #extract_and_insert_method
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
                                        return replica_ref.mirror(&other_ref);
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

fn get_extract_and_insert_method(type_name: &Ident, data: &Data) -> TokenStream {
    let variants = match *data {
        Data::Enum(ref data) => {
            let mut output = quote! {};
            for variant in data.variants.iter() {
                let variant_name = &variant.ident;
                let new_output_right = quote! {
                    #type_name::#variant_name(replica_ref) => {
                        extractor.extract(key, replica_ref.clone());
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
        fn extract_and_insert<K: EntityType, E: ProtocolExtractor<#type_name, K>>(&self,
                                      key: &K,
                                      extractor: &mut E) {
            match self {
                #variants
            }
        }
    };
}
