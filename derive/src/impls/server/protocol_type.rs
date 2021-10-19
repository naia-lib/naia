use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput};

use crate::protocol_type::*;

pub fn protocol_type_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let protocol_name = input.ident;

    let variants = get_variants(&input.data);

    let kind_enum_name = format_ident!("{}Kind", protocol_name);
    let kind_enum_def = get_kind_enum(&kind_enum_name, &variants);
    let kind_of_method = get_kind_of_method(&kind_enum_name, &variants);

    let load_method = get_load_method(&protocol_name, &input.data);
    let dyn_ref_method = get_dyn_ref_method(&protocol_name, &input.data);
    let dyn_mut_method = get_dyn_mut_method(&protocol_name, &input.data);
    let cast_method = get_cast_method(&protocol_name, &input.data);
    let cast_ref_method = get_cast_ref_method(&protocol_name, &input.data);
    let cast_mut_method = get_cast_mut_method(&protocol_name, &input.data);

    let gen = quote! {
        use std::any::{Any, TypeId};
        use naia_shared::{ProtocolType, ProtocolKindType, Replicate, ReplicateEq, DiffMask, PacketReader, EntityType, DynRef, DynMut};

        #kind_enum_def

        impl #protocol_name {
            #load_method
        }

        impl ProtocolType for #protocol_name {
            type Kind = #kind_enum_name;
            #kind_of_method

            #dyn_ref_method
            #dyn_mut_method
            #cast_method
            #cast_ref_method
            #cast_mut_method
        }
    };

    proc_macro::TokenStream::from(gen)
}