use crate::parse::Struct;

use proc_macro::TokenStream;

pub fn derive_ser_struct(struct_: &Struct) -> TokenStream {
    let mut body = String::new();

    for field in &struct_.fields {
        l!(
            body,
            "writer.write(&self.{});",
            field.field_name.as_ref().unwrap()
        );
    }
    format!(
        "impl Ser for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                {}
            }}
        }}",
        struct_.name, body
    )
        .parse()
        .unwrap()
}

pub fn derive_de_struct(struct_: &Struct) -> TokenStream {
    let mut body = String::new();

    for field in &struct_.fields {
        l!(
            body,
            "{}: reader.read()?,",
            field.field_name.as_ref().unwrap()
        );
    }

    format!(
        "impl De for {} {{
            fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::DeErr> {{
                std::result::Result::Ok(Self {{
                    {}
                }})
            }}
        }}",
        struct_.name, body
    )
        .parse()
        .unwrap()
}