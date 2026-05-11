#![cfg(feature = "client")]

use std::marker::PhantomData;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::IntoScheduleConfigs;
use naia_bevy_client::Client;
use naia_bevy_shared::SendPackets;
use naia_metrics::emit_client_connection_stats;

/// Bevy plugin that emits naia client metrics once per tick, immediately
/// after naia's [`SendPackets`] system.
///
/// Generic over the same phantom tag type `T` used by [`naia_bevy_client::Plugin<T>`]
/// and [`Client<T>`]. For single-client apps, use [`DefaultClientMetricsPlugin`].
pub struct NaiaClientMetricsPlugin<T: Send + Sync + 'static>(PhantomData<T>);

impl<T: Send + Sync + 'static> Default for NaiaClientMetricsPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for NaiaClientMetricsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, emit_client_metrics::<T>.after(SendPackets));
    }
}

fn emit_client_metrics<T: Send + Sync + 'static>(client: Client<T>) {
    if let Some(stats) = client.connection_stats() {
        emit_client_connection_stats(&stats);
    }
}
