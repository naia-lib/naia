//! Very limited rust parser
//!
//! https://doc.rust-lang.org/reference/expressions/struct-expr.html
//! https://docs.rs/syn/0.15.44/syn/enum.Type.html
//! https://ziglang.org/documentation/0.5.0/#toc-typeInfo

use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

use std::iter::Peekable;

#[derive(Debug)]
pub struct Attribute {
    pub name: String,
    pub tokens: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum Visibility {
    Public,
    Crate,
    Restricted,
    Private,
}

#[derive(Debug)]
pub struct Field {
    pub vis: Visibility,
    pub field_name: Option<String>,
    pub ty: Type,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Type {
    pub is_option: bool,
    pub path: String,
}

#[derive(Debug)]
pub struct Struct {
    pub name: String,
    pub tuple: bool,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct EnumVariant {
    pub name: String,
    pub tuple: bool,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

#[allow(dead_code)]
pub enum Data {
    Struct(Struct),
    Enum(Enum),
    Union(()),
}

pub fn next_visibility_modifier(
    source: &mut Peekable<impl Iterator<Item = TokenTree>>,
) -> Option<String> {
    if let Some(TokenTree::Ident(ident)) = source.peek() {
        if format!("{}", ident) == "pub" {
            source.next();

            // skip (crate) and alike
            if let Some(TokenTree::Group(group)) = source.peek() {
                if group.delimiter() == Delimiter::Parenthesis {
                    next_group(source);
                }
            }

            return Some("pub".to_string());
        }
    }

    return None;
}

pub fn next_punct(source: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Option<String> {
    if let Some(TokenTree::Punct(punct)) = source.peek() {
        let punct = format!("{}", punct);
        source.next();
        return Some(punct);
    }

    return None;
}

pub fn next_exact_punct(
    source: &mut Peekable<impl Iterator<Item = TokenTree>>,
    pattern: &str,
) -> Option<String> {
    if let Some(TokenTree::Punct(punct)) = source.peek() {
        let punct = format!("{}", punct);
        if punct == pattern {
            source.next();
            return Some(punct);
        }
    }

    return None;
}

pub fn next_eof<T: Iterator>(source: &mut Peekable<T>) -> Option<()> {
    if source.peek().is_none() {
        Some(())
    } else {
        None
    }
}

pub fn next_ident(source: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Option<String> {
    if let Some(TokenTree::Ident(ident)) = source.peek() {
        let ident = format!("{}", ident);
        source.next();
        Some(ident)
    } else {
        None
    }
}

pub fn next_group(source: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Option<Group> {
    if let Some(TokenTree::Group(_)) = source.peek() {
        let group = match source.next().unwrap() {
            TokenTree::Group(group) => group,
            _ => unreachable!("just checked with peek()!"),
        };
        Some(group)
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn debug_current_token(source: &mut Peekable<impl Iterator<Item = TokenTree>>) {
    println!("{:?}", source.peek());
}

fn next_type<T: Iterator<Item = TokenTree>>(mut source: &mut Peekable<T>) -> Option<Type> {
    let group = next_group(&mut source);
    if group.is_some() {
        let mut group = group.unwrap().stream().into_iter().peekable();

        let mut tuple_type = Type {
            is_option: false,
            path: "".to_string(),
        };

        while let Some(next_ty) = next_type(&mut group) {
            tuple_type.path.push_str(&format!("{}, ", next_ty.path));
        }

        return Some(tuple_type);
    }

    // read a path like a::b::c::d
    let mut ty = next_ident(&mut source)?;
    while let Some(_) = next_exact_punct(&mut source, ":") {
        let _second_colon = next_exact_punct(&mut source, ":").expect("Expecting second :");

        let next_ident = next_ident(&mut source).expect("Expecting next path part after ::");
        ty.push_str(&format!("::{}", next_ident));
    }

    let angel_bracket = next_exact_punct(&mut source, "<");
    if angel_bracket.is_some() {
        let mut generic_type = next_type(source).expect("Expecting generic argument");
        while let Some(_comma) = next_exact_punct(&mut source, ",") {
            let next_ty = next_type(source).expect("Expecting generic argument");
            generic_type.path.push_str(&format!(", {}", next_ty.path));
        }

        let _closing_bracket =
            next_exact_punct(&mut source, ">").expect("Expecting closing generic bracket");

        if ty == "Option" {
            Some(Type {
                path: generic_type.path,
                is_option: true,
            })
        } else {
            Some(Type {
                path: format!("{}<{}>", ty, generic_type.path),
                is_option: false,
            })
        }
    } else {
        Some(Type {
            path: ty,
            is_option: false,
        })
    }
}

fn next_fields(
    mut body: &mut Peekable<impl Iterator<Item = TokenTree>>,
    named: bool,
) -> Vec<Field> {
    let mut fields = vec![];

    loop {
        if next_eof(&mut body).is_some() {
            break;
        }

        let _visibility = next_visibility_modifier(&mut body);
        let field_name = if named {
            let field_name = next_ident(&mut body).expect("Field name expected");

            let _ = next_exact_punct(&mut body, ":").expect("Delimeter after field name expected");
            Some(field_name)
        } else {
            None
        };

        let ty = next_type(&mut body).expect("Expected field type");
        let _punct = next_punct(&mut body);

        fields.push(Field {
            vis: Visibility::Public,
            field_name,
            ty,
        });
    }
    fields
}

fn next_struct(mut source: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Struct {
    let struct_name = next_ident(&mut source).expect("Unnamed structs are not supported");

    let group = next_group(&mut source);
    // unit struct
    if group.is_none() {
        // skip ; at the end of struct like this: "struct Foo;"
        let _ = next_punct(&mut source);

        return Struct {
            name: struct_name,
            fields: vec![],
            tuple: true,
        };
    };
    let group = group.unwrap();
    let delimiter = group.delimiter();
    let tuple = match delimiter {
        Delimiter::Parenthesis => true,
        Delimiter::Brace => false,

        _ => panic!("Struct with unsupported delimiter"),
    };

    let mut body = group.stream().into_iter().peekable();
    let fields = next_fields(&mut body, !tuple);

    if tuple == true {
        next_exact_punct(&mut source, ";").expect("Expected ; on the end of tuple struct");
    }

    Struct {
        name: struct_name,
        tuple,
        fields,
    }
}

fn next_enum(mut source: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Enum {
    let enum_name = next_ident(&mut source).expect("Unnamed enums are not supported");

    let group = next_group(&mut source);
    // unit enum
    if group.is_none() {
        return Enum {
            name: enum_name,
            variants: vec![],
        };
    };
    let group = group.unwrap();
    let mut body = group.stream().into_iter().peekable();

    let mut variants = vec![];
    loop {
        if next_eof(&mut body).is_some() {
            break;
        }

        let variant_name = next_ident(&mut body).expect("Unnamed variants are not supported");
        let group = next_group(&mut body);
        if group.is_none() {
            variants.push(EnumVariant {
                name: variant_name,
                tuple: true,
                fields: vec![],
            });
            let _maybe_comma = next_exact_punct(&mut body, ",");
            continue;
        }
        let group = group.unwrap();
        let delimiter = group.delimiter();
        let tuple = match delimiter {
            Delimiter::Parenthesis => true,
            Delimiter::Brace => false,

            _ => panic!("Enum with unsupported delimiter"),
        };
        {
            let mut body = group.stream().into_iter().peekable();
            let fields = next_fields(&mut body, !tuple);
            variants.push(EnumVariant {
                name: variant_name,
                tuple,
                fields,
            });
        }
        let _maybe_semicolon = next_exact_punct(&mut body, ";");
        let _maybe_coma = next_exact_punct(&mut body, ",");
    }

    Enum {
        name: enum_name,
        variants,
    }
}

pub fn parse_data(input: TokenStream) -> Data {
    let mut source = input.into_iter().peekable();

    let pub_or_type = next_ident(&mut source).expect("Not an ident");

    let type_keyword = if pub_or_type == "pub" {
        next_ident(&mut source).expect("pub(whatever) is not supported yet")
    } else {
        pub_or_type
    };

    let res;

    match type_keyword.as_str() {
        "struct" => {
            res = Data::Struct(next_struct(&mut source));
        }
        "enum" => {
            let enum_ = next_enum(&mut source);
            res = Data::Enum(enum_);
        }
        "union" => unimplemented!("Unions are not supported"),
        unexpected => panic!("Unexpected keyword: {}", unexpected),
    }

    assert!(
        source.next().is_none(),
        "Unexpected data after end of the struct"
    );

    res
}
