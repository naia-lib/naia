// Server aggregate metrics (no label)
/// Gauge name for the current count of connected users on the server.
pub const SERVER_CONNECTED_USERS: &str = "naia_server_connected_users";
/// Gauge name for the current total entity count on the server.
pub const SERVER_TOTAL_ENTITIES: &str = "naia_server_total_entities";
/// Gauge name for the current total room count on the server.
pub const SERVER_TOTAL_ROOMS: &str = "naia_server_total_rooms";

// Server per-connection metrics (label: user_id)
/// Gauge name for a connection's round-trip time, labeled by `user_id`.
pub const SERVER_CONN_RTT_MS: &str = "naia_server_conn_rtt_ms";
/// Gauge name for a connection's 99th-percentile round-trip time, labeled by `user_id`.
pub const SERVER_CONN_RTT_P99_MS: &str = "naia_server_conn_rtt_p99_ms";
/// Gauge name for a connection's RTT jitter, labeled by `user_id`.
pub const SERVER_CONN_JITTER_MS: &str = "naia_server_conn_jitter_ms";
/// Gauge name for a connection's packet loss percentage, labeled by `user_id`.
pub const SERVER_CONN_PACKET_LOSS: &str = "naia_server_conn_packet_loss";
/// Gauge name for a connection's outbound kilobits-per-second, labeled by `user_id`.
pub const SERVER_CONN_KBPS_SENT: &str = "naia_server_conn_kbps_sent";
/// Gauge name for a connection's inbound kilobits-per-second, labeled by `user_id`.
pub const SERVER_CONN_KBPS_RECV: &str = "naia_server_conn_kbps_recv";

// Server replication counters (no label — server-wide totals)
pub use naia_shared::{
    MESSAGES_SENT_TOTAL, SERVER_COMPONENT_INSERTS_TOTAL, SERVER_COMPONENT_REMOVES_TOTAL,
    SERVER_DESPAWNS_TOTAL, SERVER_SPAWNS_TOTAL,
};

// Client connection metrics (no label — one connection per process)
/// Gauge name for the client's round-trip time.
pub const CLIENT_CONN_RTT_MS: &str = "naia_client_conn_rtt_ms";
/// Gauge name for the client's 99th-percentile round-trip time.
pub const CLIENT_CONN_RTT_P99_MS: &str = "naia_client_conn_rtt_p99_ms";
/// Gauge name for the client's RTT jitter.
pub const CLIENT_CONN_JITTER_MS: &str = "naia_client_conn_jitter_ms";
/// Gauge name for the client's packet loss percentage.
pub const CLIENT_CONN_PACKET_LOSS: &str = "naia_client_conn_packet_loss";
/// Gauge name for the client's outbound kilobits-per-second.
pub const CLIENT_CONN_KBPS_SENT: &str = "naia_client_conn_kbps_sent";
/// Gauge name for the client's inbound kilobits-per-second.
pub const CLIENT_CONN_KBPS_RECV: &str = "naia_client_conn_kbps_recv";
