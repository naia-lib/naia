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

use state::state_impl;
use state_type::state_type_impl;

/// Derives the StateType trait for a given enum
#[proc_macro_derive(StateType)]
pub fn state_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    state_type_impl(input)
}

/// Derives the State trait for a given struct
#[proc_macro_derive(State, attributes(type_name))]
pub fn state_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    state_impl(input)
}
