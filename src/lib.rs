mod entity_type;
use entity_type::entity_type_impl;

#[proc_macro_derive(EntityType)]
pub fn entity_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity_type_impl(input)
}