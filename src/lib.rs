extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn;

#[proc_macro_derive(EntityType)]
pub fn entity_type_derive(input: TokenStream) -> TokenStream {

    // Construct a representation of Rust code as a syntax tree
    // that we can manipulate
    let ast = syn::parse(input).unwrap();

    // Build the trait implementation
    entity_type_impl(&ast)
}

fn entity_type_impl(ast: &syn::DeriveInput) -> TokenStream {

    let type_name = &ast.ident;

    let gen = quote! {
        impl EntityType for #type_name {
            fn read_partial(&mut self, state_mask: &StateMask, bytes: &[u8]) {
                match self {
                    #type_name::PointEntity(identity) => {
                        identity.as_ref().borrow_mut().read_partial(state_mask, bytes);
                    }
                }
            }
        }
    };
    gen.into()
}