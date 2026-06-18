use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput};

pub fn client_marker_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let m = &input.ident;

    // All generated names follow {Marker}{Suffix} via format_ident! (no paste).
    let client = format_ident!("{}Client", m);
    let connect_event = format_ident!("{}ConnectEvent", m);
    let disconnect_event = format_ident!("{}DisconnectEvent", m);
    let reject_event = format_ident!("{}RejectEvent", m);
    let spawn_entity_event = format_ident!("{}SpawnEntityEvent", m);
    let despawn_entity_event = format_ident!("{}DespawnEntityEvent", m);
    let error_event = format_ident!("{}ErrorEvent", m);
    let message_events = format_ident!("{}MessageEvents", m);
    let request_events = format_ident!("{}RequestEvents", m);
    let client_tick_event = format_ident!("{}ClientTickEvent", m);
    let server_tick_event = format_ident!("{}ServerTickEvent", m);
    let insert_component_event = format_ident!("{}InsertComponentEvent", m);
    let insert_bundle_event = format_ident!("{}InsertBundleEvent", m);
    let update_component_event = format_ident!("{}UpdateComponentEvent", m);
    let remove_component_event = format_ident!("{}RemoveComponentEvent", m);
    let app_bundle_ext = format_ident!("{}AppBundleExt", m);

    let gen: TokenStream = quote! {
        pub type #client<'w>                = diax_net_client::Client<'w, #m>;
        pub type #connect_event             = diax_net_client::events::ConnectEvent<#m>;
        pub type #disconnect_event          = diax_net_client::events::DisconnectEvent<#m>;
        pub type #reject_event              = diax_net_client::events::RejectEvent<#m>;
        pub type #spawn_entity_event        = diax_net_client::events::SpawnEntityEvent<#m>;
        pub type #despawn_entity_event      = diax_net_client::events::DespawnEntityEvent<#m>;
        pub type #error_event               = diax_net_client::events::ErrorEvent<#m>;
        pub type #message_events            = diax_net_client::events::MessageEvents<#m>;
        pub type #request_events            = diax_net_client::events::RequestEvents<#m>;
        pub type #client_tick_event         = diax_net_client::events::ClientTickEvent<#m>;
        pub type #server_tick_event         = diax_net_client::events::ServerTickEvent<#m>;
        pub type #insert_component_event<C> = diax_net_client::events::InsertComponentEvent<#m, C>;
        pub type #insert_bundle_event<B>    = diax_net_client::events::InsertBundleEvent<#m, B>;
        pub type #update_component_event<C> = diax_net_client::events::UpdateComponentEvent<#m, C>;
        pub type #remove_component_event<C> = diax_net_client::events::RemoveComponentEvent<#m, C>;

        pub trait #app_bundle_ext {
            fn add_bundle_events<B: diax_net_client::ReplicateBundle>(&mut self) -> &mut Self;
            fn add_world_component_events<C: diax_net_client::Replicate>(&mut self) -> &mut Self;
        }

        impl #app_bundle_ext for bevy_app::App {
            fn add_bundle_events<B: diax_net_client::ReplicateBundle>(&mut self) -> &mut Self {
                diax_net_client::AppRegisterComponentEvents::add_bundle_events::<#m, B>(self);
                self
            }
            fn add_world_component_events<C: diax_net_client::Replicate>(&mut self) -> &mut Self {
                diax_net_client::AppRegisterComponentEvents::add_component_events::<#m, C>(self);
                self
            }
        }
    };

    proc_macro::TokenStream::from(gen)
}
