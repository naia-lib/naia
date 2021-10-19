use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, Ident};

pub fn get_trait_impl_methods(_protocol_name: &Ident, _data: &Data) -> TokenStream {
    return quote! {};
}