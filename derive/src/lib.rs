//! # Naia Derive
//! Procedural macros to simplify implementation of Naia Replicate &
//! ProtocolType traits

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(all(feature = "client", feature = "server"))]
    {
        // Use both protocols...
        compile_error!("naia-derive requires either the 'client' OR 'server' feature to be enabled, you must pick one.");
    }
    else if #[cfg(all(not(feature = "client"), not(feature = "server")))]
    {
        // Use no protocols...
        compile_error!("naia-derive requires either the 'client' OR 'server' feature to be enabled, you must pick one.");
    }
}

mod protocol_type;
mod replicate;

use protocol_type::protocol_type_impl;
use replicate::replicate_impl;

/// Derives the ProtocolType trait for a given enum
#[proc_macro_derive(ProtocolType)]
pub fn protocol_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    protocol_type_impl(input)
}

/// Derives the Replicate trait for a given struct
#[proc_macro_derive(Replicate, attributes(protocol_path))]
pub fn replicate_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    replicate_impl(input)
}
