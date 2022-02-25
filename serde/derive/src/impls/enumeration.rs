use crate::parse::Enum;

use proc_macro::TokenStream;

pub fn derive_ser_enum(enum_: &Enum) -> TokenStream {
    let mut r = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let lit = format!("{}u16", index);
        let ident = &variant.name;
        // Unit
        if variant.fields.len() == 0 {
            l!(r, "Self::{} => {}.ser(s),", ident, lit);
        }
        // Named
        else if variant.named {
            l!(r, "Self::{} {{", variant.name);
            for field in &variant.fields {
                l!(r, "{}, ", field.field_name.as_ref().unwrap());
            }
            l!(r, "} => {");
            l!(r, "{}.ser(s);", lit);
            for field in &variant.fields {
                l!(r, "{}.ser(s);", field.field_name.as_ref().unwrap());
            }
            l!(r, "}");
        }
        // Unnamed
        else if variant.named == false {
            l!(r, "Self::{} (", variant.name);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(r, "f{}, ", n);
            }
            l!(r, ") => {");
            l!(r, "{}.ser(s);", lit);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(r, "f{}.ser(s);", n);
            }
            l!(r, "}");
        }
    }

    format!(
        "impl Ser for {} {{
            fn ser(&self, writer: &mut BitWriter) {{
                match self {{
                  {}
                }}
            }}
        }}",
        enum_.name, r
    )
        .parse()
        .unwrap()
}

pub fn derive_de_enum(enum_: &Enum) -> TokenStream {
    let mut r = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let lit = format!("{}u16", index);

        // Unit
        if variant.fields.len() == 0 {
            l!(r, "{} => Self::{},", lit, variant.name);
        }
        // Named
        else if variant.named {
            l!(r, "{} => Self::{} {{", lit, variant.name);
            for field in &variant.fields {
                l!(
                    r,
                    "{}: De::de(o, d)?,",
                    field.field_name.as_ref().unwrap()
                );
            }
            l!(r, "},");
        }
        // Unnamed
        else if variant.named == false {
            l!(r, "{} => Self::{} (", lit, variant.name);
            for _ in &variant.fields {
                l!(r, "De::de(o, d)?,");
            }
            l!(r, "),");
        }
    }

    format!(
        "impl  De for {} {{
            fn de(reader: &mut BitReader) -> std::result::Result<Self, DeErr> {{
                let id: u16 = De::de(o,d)?;
                Ok(match id {{
                    {}
                    _ => return std::result::Result::Err(DeErr{{o:*o, l:0, s:d.len()}})
                }})
            }}
        }}", enum_.name, r)
        .parse()
        .unwrap()
}