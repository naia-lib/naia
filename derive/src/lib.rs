//! # Naia Derive
//! Procedural macros to simplify implementation of Naia ReplicateSafe &
//! ProtocolType traits

#![deny(trivial_casts, trivial_numeric_casts, unstable_features)]

mod protocol_type;
mod replicate;

use protocol_type::protocol_type_impl;
use replicate::replicate_impl;

/// Derives the ProtocolType trait for a given enum
#[proc_macro_derive(ProtocolType)]
pub fn protocol_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    protocol_type_impl(input)
}

/// Derives the ReplicateSafe trait for a given struct
#[proc_macro_derive(ReplicateSafe, attributes(protocol_path))]
pub fn replicate_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    replicate_impl(input)
}
