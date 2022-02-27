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

    let name = &struct_.name;

    format!(
        "
        mod impl_serde_{name} {{
            use super::serde::*;
            use super::{name};
            impl Serde for {name} {{
                fn ser<S: BitWrite>(&self, writer: &mut S) {{
                    {ser_body}
                }}
                fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::SerdeErr> {{
                    std::result::Result::Ok(Self {{
                        {de_body}
                    }})
                }}
            }}
        }}
        "
    )
}
