use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type};

pub mod shared;

cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::replicate_impl;
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::replicate_impl;
    }
}