use crate::parse::Enum;

use proc_macro::TokenStream;

fn bits_needed_for(max_value: usize) -> u8 {
    let mut bits = 1;
    while (2 as usize).pow(bits) <= max_value {
        bits += 1;
    }
    if bits >= 256 {
        panic!("cannot encode a number in more than 255 bits!");
    }
    return bits as u8;
}

pub fn derive_ser_enum(enum_: &Enum) -> TokenStream {

    let variant_number = enum_.variants.len();
    let bits_needed = bits_needed_for(variant_number);

    let mut variants = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {

        let variant_name = &variant.name;

        // Unit Variant
        if variant.fields.len() == 0 {
            l!(variants, "Self::{} => {{", variant_name);

            // INDEX
            l!(variants, "let index = UnsignedInteger::<{}>::new({});", bits_needed, index);
            l!(variants, "writer.write(&index);");

            l!(variants, "},");
        }
        // Struct Variant
        else if variant.tuple == false {
            l!(variants, "Self::{} {{", variant.name);
            for field in &variant.fields {
                l!(variants, "{}, ", field.field_name.as_ref().unwrap());
            }
            l!(variants, "} => {");

            // INDEX
            l!(variants, "let index = UnsignedInteger::<{}>::new({});", bits_needed, index);
            l!(variants, "writer.write(&index);");

            for field in &variant.fields {
                l!(variants, "writer.write({});", field.field_name.as_ref().unwrap());
            }
            l!(variants, "}");
        }
        // Tuple Variant
        else if variant.tuple == true {
            l!(variants, "Self::{} (", variant.name);
            for (n, _) in variant.fields.iter().enumerate() {
                l!(variants, "f{}, ", n);
            }
            l!(variants, ") => {");

            // INDEX
            l!(variants, "let index = UnsignedInteger::<{}>::new({});", bits_needed, index);
            l!(variants, "writer.write(&index);");

            for (n, _) in variant.fields.iter().enumerate() {
                l!(variants, "writer.write(f{});", n);
            }
            l!(variants, "}");
        }
    }

    let name = enum_.name.clone();

    format!(
        "
        mod {name}_ser {{
            use naia_serde::{{UnsignedInteger, Ser, BitWriter}};
            use super::{name};
            impl Ser for {name} {{
                fn ser(&self, writer: &mut BitWriter) {{
                    match self {{
                      {variants}
                    }}
                }}
            }}
        }}
        "
    )
        .parse()
        .unwrap()
}

pub fn derive_de_enum(enum_: &Enum) -> TokenStream {

    let variant_number = enum_.variants.len();
    let bits_needed = bits_needed_for(variant_number);

    let mut variants = String::new();

    for (index, variant) in enum_.variants.iter().enumerate() {
        let variant_index = format!("{}u16", index);

        // Unit Variant
        if variant.fields.len() == 0 {
            l!(variants, "{} => Self::{},", variant_index, variant.name);
        }
        // Struct Variant
        else if variant.tuple == false {
            l!(variants, "{} => Self::{} {{", variant_index, variant.name);
            for field in &variant.fields {
                l!(
                    variants,
                    "{}: reader.read()?,",
                    field.field_name.as_ref().unwrap()
                );
            }
            l!(variants, "},");
        }
        // Tuple Variant
        else if variant.tuple == true {
            l!(variants, "{} => Self::{} (", variant_index, variant.name);
            for _ in &variant.fields {
                l!(variants, "reader.read()?,");
            }
            l!(variants, "),");
        }
    }

    let name = enum_.name.clone();

    format!(
        "
        mod {name}_de {{
            use naia_serde::{{UnsignedInteger, De, BitReader}};
            use super::{name};
            impl  De for {name} {{
                fn de(reader: &mut BitReader) -> std::result::Result<Self, naia_serde::DeErr> {{
                    let index: UnsignedInteger<{bits_needed}> = reader.read().unwrap();
                    let index_u16: u16 = index.get() as u16;
                    Ok(match index_u16 {{
                        {variants}
                        _ => return std::result::Result::Err(naia_serde::DeErr{{}})
                    }})
                }}
            }}
        }}")
        .parse()
        .unwrap()
}