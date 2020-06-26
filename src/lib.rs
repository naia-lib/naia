mod entity_type;
mod event_type;
mod event;
mod entity;
mod utils;

use entity_type::entity_type_impl;
use event_type::event_type_impl;
use event::event_impl;
use entity::entity_impl;

#[proc_macro_derive(EntityType)]
pub fn entity_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity_type_impl(input)
}

#[proc_macro_derive(EventType)]
pub fn event_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    event_type_impl(input)
}

#[proc_macro_derive(Event, attributes(type_name))]
pub fn event_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    event_impl(input)
}

#[proc_macro_derive(Entity, attributes(type_name))]
pub fn entity_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity_impl(input)
}