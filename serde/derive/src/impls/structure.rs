use crate::parse::Struct;

use proc_macro::TokenStream;

pub fn derive_serde_struct(struct_: &Struct) -> TokenStream {
    let mut ser_body = String::new();
    let mut de_body = String::new();

    for field in &struct_.fields {
        l!(
            ser_body,
            "writer.write(&self.{});",
            field.field_name.as_ref().unwrap()
        );
    }

    for field in &struct_.fields {
        l!(
            de_body,
            "{}: reader.read()?,",
            field.field_name.as_ref().unwrap()
        );
    }

    format!(
        "impl Serde for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                {}
            }}
            fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::SerdeErr> {{
                std::result::Result::Ok(Self {{
                    {}
                }})
            }}
        }}",
        struct_.name, ser_body, de_body
    )
        .parse()
        .unwrap()
}