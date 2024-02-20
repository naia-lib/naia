use std::{marker::PhantomData, ops::DerefMut, sync::Mutex};

use bevy_app::{App, Plugin as PluginType, Update};
use bevy_ecs::{entity::Entity, schedule::IntoSystemConfigs};

use crate::events::RequestEvents;
use naia_bevy_shared::{BeforeReceiveEvents, Protocol, SharedPlugin, WorldData};
use naia_client::{Client, ClientConfig};

use super::{
    client::ClientWrapper,
    events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
        EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, InsertComponentEvents,
        MessageEvents, PublishEntityEvent, RejectEvent, RemoveComponentEvents, ServerTickEvent,
        SpawnEntityEvent, UnpublishEntityEvent, UpdateComponentEvents,
    },
    systems::before_receive_events,
};

struct PluginConfig {
    client_config: ClientConfig,
    protocol: Protocol,
}

impl PluginConfig {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        PluginConfig {
            client_config,
            protocol,
        }
    }
}

pub struct Plugin<T> {
    config: Mutex<Option<PluginConfig>>,
    phantom_t: PhantomData<T>,
}

impl<T> Plugin<T> {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        let config = PluginConfig::new(client_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
            phantom_t: PhantomData,
        }
    }
}

impl<T: Sync + Send + 'static> PluginType for Plugin<T> {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().unwrap().deref_mut().take().unwrap();

        let mut world_data = config.protocol.take_world_data();
        world_data.add_systems(app);

        if let Some(old_world_data) = app.world.remove_resource::<WorldData>() {
            world_data.merge(old_world_data);
        }

        app.insert_resource(world_data);

        let client = Client::<Entity>::new(config.client_config, config.protocol.into());
        let client = ClientWrapper::<T>::new(client);

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<T>::new())
            // RESOURCES //
            .insert_resource(client)
            // EVENTS //
            .add_event::<ConnectEvent<T>>()
            .add_event::<DisconnectEvent<T>>()
            .add_event::<RejectEvent<T>>()
            .add_event::<ErrorEvent<T>>()
            .add_event::<MessageEvents<T>>()
            .add_event::<RequestEvents<T>>()
            .add_event::<ClientTickEvent<T>>()
            .add_event::<ServerTickEvent<T>>()
            .add_event::<SpawnEntityEvent<T>>()
            .add_event::<DespawnEntityEvent<T>>()
            .add_event::<PublishEntityEvent<T>>()
            .add_event::<UnpublishEntityEvent<T>>()
            .add_event::<EntityAuthGrantedEvent<T>>()
            .add_event::<EntityAuthDeniedEvent<T>>()
            .add_event::<EntityAuthResetEvent<T>>()
            .add_event::<InsertComponentEvents<T>>()
            .add_event::<UpdateComponentEvents<T>>()
            .add_event::<RemoveComponentEvents<T>>()
            // SYSTEMS //
            .add_systems(
                Update,
                before_receive_events::<T>.in_set(BeforeReceiveEvents),
            );
    }
}
