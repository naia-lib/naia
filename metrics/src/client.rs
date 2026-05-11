use naia_shared::ConnectionStats;
use crate::names;

/// Emit the five client-side connection gauges.
///
/// The client has exactly one connection, so no label is needed.
/// Call once per tick after [`Client::send_all_packets`].
pub fn emit_client_connection_stats(stats: &ConnectionStats) {
    metrics::gauge!(names::CLIENT_CONN_RTT_MS).set(stats.rtt_ms as f64);
    metrics::gauge!(names::CLIENT_CONN_RTT_P99_MS).set(stats.rtt_p99_ms as f64);
    metrics::gauge!(names::CLIENT_CONN_JITTER_MS).set(stats.jitter_ms as f64);
    metrics::gauge!(names::CLIENT_CONN_PACKET_LOSS).set(stats.packet_loss_pct as f64);
    metrics::gauge!(names::CLIENT_CONN_KBPS_SENT).set(stats.kbps_sent as f64);
    metrics::gauge!(names::CLIENT_CONN_KBPS_RECV).set(stats.kbps_recv as f64);
}
