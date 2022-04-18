extern crate proc_macro;

use naia_parse::parse;

#[macro_use]
mod shared;
mod impls;

use impls::*;

#[proc_macro_attribute]
pub fn derive_serde(
    _: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let define_string = input.to_string();

    let input = parse::parse_data(input);

    // ok we have an ident, its either a struct or a enum
    let impl_string = match &input {
        parse::Data::Struct(struct_) if struct_.tuple => derive_serde_tuple_struct(struct_),
        parse::Data::Struct(struct_) => derive_serde_struct(struct_),
        parse::Data::Enum(enum_) => derive_serde_enum(enum_),
        _ => unimplemented!("Only structs and enums are supported"),
    };

    let output = format!(
        "
        #[derive(PartialEq, Clone)]
        {define_string}
        {impl_string}
        "
    )
    .parse()
    .unwrap();

    output
}
