extern crate proc_macro;

use naia_parse::{parse, parse::Enum};

pub fn channels_impl(
    _: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let define_string = input.to_string();

    let input = parse::parse_data(input);

    if let parse::Data::Enum(enum_) = &input {
        // ok we have an ident, its either a struct or a enum
        let impl_string = derive_channel_enum(enum_);
        let enum_name = enum_.name.clone();

        let output = format!(
            "
            mod define_{enum_name} {{
                use naia_shared::{{derive_serde, serde, ChannelIndex}};
                #[derive(Hash, Eq)]
                #[derive_serde]
                {define_string}
                {impl_string}
            }}
            pub use define_{enum_name}::{enum_name};
            "
        )
        .parse()
        .unwrap();

        output
    } else {
        unimplemented!("Only enums are supported");
    }
}

pub fn derive_channel_enum(enum_: &Enum) -> String {
    let enum_name = enum_.name.clone();
    format!(
        "
        impl ChannelIndex for {enum_name} {{}}
        "
    )
    .parse()
    .unwrap()
}
