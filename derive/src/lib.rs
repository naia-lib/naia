//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Event & State traits

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

mod state;
mod state_type;
mod event;
mod event_type;

use state::state_impl;
use state_type::state_type_impl;
use event::event_impl;
use event_type::event_type_impl;

/// Derives the StateType trait for a given enum
#[proc_macro_derive(StateType)]
pub fn state_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    state_type_impl(input)
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

/// Derives the State trait for a given struct
#[proc_macro_derive(State, attributes(type_name))]
pub fn state_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    state_impl(input)
}
