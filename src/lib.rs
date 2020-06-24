extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_derive(EventType)]
pub fn derive_event_type(_item: TokenStream) -> TokenStream {
    "fn answer() -> u32 { 42 }".parse().unwrap()
}