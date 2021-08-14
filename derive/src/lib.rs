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
mod protocol_type;

use state::state_impl;
use protocol_type::protocol_type_impl;

/// Derives the ProtocolType trait for a given enum
#[proc_macro_derive(ProtocolType)]
pub fn protocol_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    protocol_type_impl(input)
}

/// Derives the State trait for a given struct
#[proc_macro_derive(State, attributes(type_name))]
pub fn state_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    state_impl(input)
}
