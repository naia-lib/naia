use crate::parse::Struct;

pub fn derive_serde_struct(struct_: &Struct) -> String {
    let mut ser_body = String::new();
    let mut de_body = String::new();

    for field in &struct_.fields {
        l!(
            ser_body,
            "self.{}.ser(writer);",
            field.field_name.as_ref().unwrap()
        );
    }

    for field in &struct_.fields {
        l!(
            de_body,
            "{}: Serde::de(reader)?,",
            field.field_name.as_ref().unwrap()
        );
    }

    format!(
        "impl Serde for {} {{
            fn ser<S: BitWrite>(&self, writer: &mut S) {{
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
}
