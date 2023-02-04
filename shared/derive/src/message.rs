use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, GenericArgument, Ident, Index, Lit, LitStr,
    Member, Meta, Path, PathArguments, Result, Type,
};

use super::{shared::{get_struct_type, StructType}};

pub fn message_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Helper Properties
    let struct_type = get_struct_type(&input);
    let properties = properties(&input);

    // Names
    let struct_name = input.ident;
    let struct_name_str = LitStr::new(&struct_name.to_string(), struct_name.span());

    // Methods
    let clone_method = clone_method(&struct_name, &properties, &struct_type);

    let gen = quote! {

        mod internal {
            pub use std::any::Any;
            pub use naia_shared::Named;
        }

        impl Message for #struct_name {
            fn into_any(self: Box<Self>) -> Box<dyn internal::Any> {
                self
            }
        }
        impl internal::Named for #struct_name {
            fn name(&self) -> String {
                return #struct_name_str.to_string();
            }
        }
        impl Clone for #struct_name {
            #clone_method
        }
    };

    proc_macro::TokenStream::from(gen)
}

fn clone_method(
    replica_name: &Ident,
    properties: &[Property],
    struct_type: &StructType,
) -> TokenStream {
    let mut output = quote! {};
    let mut entity_property_output = quote! {};

    for (index, property) in properties.iter().enumerate() {
        let field_name = get_field_name(property, index, struct_type);
        match property {
            Property::NonReplicated(_) | Property::Entity(_) => {
                let new_output_right = quote! {
                    #field_name: self.#field_name.clone(),
                };
                let new_output_result = quote! {
                    #output
                    #new_output_right
                };
                output = new_output_result;
            }
        };
    }

    quote! {
        fn clone(&self) -> #replica_name {
            let mut new_clone = #replica_name {
                #output
            };
            return new_clone;
        }
    }
}

fn properties(input: &DeriveInput) -> Vec<Property> {
    let mut fields = Vec::new();

    if let Data::Struct(data_struct) = &input.data {
        match &data_struct.fields {
            Fields::Named(fields_named) => {
                for field in fields_named.named.iter() {
                    if let Some(variable_name) = &field.ident {
                        if let Type::Path(type_path) = &field.ty {
                            if let Some(property_seg) = type_path.path.segments.first() {
                                let property_type = property_seg.ident.clone();
                                // EntityProperty
                                if property_type == "EntityProperty" {
                                    fields.push(Property::entity(variable_name.clone()));
                                    continue;
                                    // Property
                                } else {
                                    fields.push(Property::nonreplicated(
                                        variable_name.clone(),
                                        field.ty.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            Fields::Unnamed(fields_unnamed) => {
                for (index, field) in fields_unnamed.unnamed.iter().enumerate() {
                    if let Type::Path(type_path) = &field.ty {
                        if let Some(property_seg) = type_path.path.segments.first() {
                            let property_type = property_seg.ident.clone();
                            let variable_name =
                                get_variable_name_for_unnamed_field(index, property_type.span());
                            if property_type == "EntityProperty" {
                                fields.push(Property::entity(variable_name));
                                continue;
                            } else {
                                fields.push(Property::nonreplicated(variable_name, field.ty.clone()))
                            }
                        }
                    }
                }
            }
            Fields::Unit => {}
        }
    } else {
        panic!("Can only derive Replicate on a struct");
    }

    fields
}

/// Get the field name as a TokenStream
fn get_field_name(property: &Property, index: usize, struct_type: &StructType) -> Member {
    match *struct_type {
        StructType::Struct => Member::from(property.variable_name().clone()),
        StructType::TupleStruct => {
            let index = Index {
                index: index as u32,
                span: property.variable_name().span(),
            };
            Member::from(index)
        }
        _ => {
            panic!("The struct should not have any fields")
        }
    }
}

const UNNAMED_FIELD_PREFIX: &'static str = "unnamed_field_";
fn get_variable_name_for_unnamed_field(index: usize, span: Span) -> Ident {
    Ident::new(&format!("{}{}", UNNAMED_FIELD_PREFIX, index), span)
}

pub struct EntityProperty {
    pub variable_name: Ident,
    pub uppercase_variable_name: Ident,
}

pub struct NonReplicatedProperty {
    pub variable_name: Ident,
    pub field_type: Type,
}

#[allow(clippy::large_enum_variant)]
pub enum Property {
    Entity(EntityProperty),
    NonReplicated(NonReplicatedProperty),
}

impl Property {

    pub fn entity(variable_name: Ident) -> Self {
        Self::Entity(EntityProperty {
            variable_name: variable_name.clone(),
            uppercase_variable_name: Ident::new(
                variable_name.to_string().to_uppercase().as_str(),
                Span::call_site(),
            ),
        })
    }

    pub fn nonreplicated(variable_name: Ident, field_type: Type) -> Self {
        Self::NonReplicated(NonReplicatedProperty {
            variable_name: variable_name.clone(),
            field_type,
        })
    }

    pub fn variable_name(&self) -> &Ident {
        match self {
            Self::Entity(property) => &property.variable_name,
            Self::NonReplicated(property) => &property.variable_name,
        }
    }

    pub fn uppercase_variable_name(&self) -> &Ident {
        match self {
            Self::Entity(property) => &property.uppercase_variable_name,
            Self::NonReplicated(_) => panic!("Unused for non-replicated properties"),
        }
    }
}