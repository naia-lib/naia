#![cfg(feature = "server")]

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::IntoScheduleConfigs;
use naia_bevy_server::{BigMapKey, Server};
use naia_bevy_shared::SendPackets;
use naia_metrics::{emit_server_aggregates, emit_server_connection_stats};

/// Bevy plugin that emits naia server metrics once per tick, immediately
/// after naia's [`SendPackets`] system.
pub struct NaiaServerMetricsPlugin;

impl Plugin for NaiaServerMetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, emit_server_metrics.after(SendPackets));
    }
}

fn emit_server_metrics(server: Server) {
    emit_server_aggregates(
        server.user_count(),
        server.entity_count(),
        server.room_count(),
    );
    for user_key in server.user_keys() {
        if let Some(stats) = server.connection_stats(&user_key) {
            emit_server_connection_stats(&stats, user_key.to_u64());
        }
    }
}
