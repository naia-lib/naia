use naia_shared::ConnectionStats;
use crate::names;

/// Emit the three server-wide aggregate gauges.
///
/// Call once per tick after [`Server::send_all_packets`].
pub fn emit_server_aggregates(user_count: usize, entity_count: usize, room_count: usize) {
    metrics::gauge!(names::SERVER_CONNECTED_USERS).set(user_count as f64);
    metrics::gauge!(names::SERVER_TOTAL_ENTITIES).set(entity_count as f64);
    metrics::gauge!(names::SERVER_TOTAL_ROOMS).set(room_count as f64);
}

/// Emit the six per-connection gauges for one user.
///
/// `user_id` is `UserKey::to_u64()`. Call once per connected user per tick.
pub fn emit_server_connection_stats(stats: &ConnectionStats, user_id: u64) {
    let id = user_id.to_string();
    metrics::gauge!(names::SERVER_CONN_RTT_MS,      "user_id" => id.clone()).set(stats.rtt_ms as f64);
    metrics::gauge!(names::SERVER_CONN_RTT_P99_MS,  "user_id" => id.clone()).set(stats.rtt_p99_ms as f64);
    metrics::gauge!(names::SERVER_CONN_JITTER_MS,   "user_id" => id.clone()).set(stats.jitter_ms as f64);
    metrics::gauge!(names::SERVER_CONN_PACKET_LOSS, "user_id" => id.clone()).set(stats.packet_loss_pct as f64);
    metrics::gauge!(names::SERVER_CONN_KBPS_SENT,   "user_id" => id.clone()).set(stats.kbps_sent as f64);
    metrics::gauge!(names::SERVER_CONN_KBPS_RECV,   "user_id" => id).set(stats.kbps_recv as f64);
}
