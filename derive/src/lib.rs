//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Event & Actor traits

#![deny(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces
)]

#[macro_use]
extern crate cfg_if;

mod actor;
mod actor_type;
mod event;
mod event_type;
mod utils;

use actor::actor_impl;
use actor_type::actor_type_impl;
use event::event_impl;
use event_type::event_type_impl;

/// Derives the ActorType trait for a given enum
#[proc_macro_derive(ActorType)]
pub fn actor_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    actor_type_impl(input)
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

/// Derives the Actor trait for a given struct
#[proc_macro_derive(Actor, attributes(type_name))]
pub fn actor_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    actor_impl(input)
}
