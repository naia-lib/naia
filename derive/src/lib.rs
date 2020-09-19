//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Event & Entity traits

#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

mod entity;
mod entity_type;
mod event;
mod event_type;
mod utils;

use entity::entity_impl;
use entity_type::entity_type_impl;
use event::event_impl;
use event_type::event_type_impl;

/// Derives the EntityType trait for a given enum
#[proc_macro_derive(EntityType)]
pub fn entity_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity_type_impl(input)
}

/// Derives the EventType trait for a given enum
#[proc_macro_derive(EventType)]
pub fn event_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    event_type_impl(input)
}

/// Derives the Event trait for a given struct
#[proc_macro_derive(Event, attributes(type_name))]
pub fn event_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    event_impl(input)
}

/// Derives the Entity trait for a given struct
#[proc_macro_derive(Entity, attributes(type_name, interpolate))]
pub fn entity_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    entity_impl(input)
}
