// Server aggregate metrics (no label)
pub const SERVER_CONNECTED_USERS: &str = "naia_server_connected_users";
pub const SERVER_TOTAL_ENTITIES:  &str = "naia_server_total_entities";
pub const SERVER_TOTAL_ROOMS:     &str = "naia_server_total_rooms";

// Server per-connection metrics (label: user_id)
pub const SERVER_CONN_RTT_MS:      &str = "naia_server_conn_rtt_ms";
pub const SERVER_CONN_RTT_P99_MS:  &str = "naia_server_conn_rtt_p99_ms";
pub const SERVER_CONN_JITTER_MS:   &str = "naia_server_conn_jitter_ms";
pub const SERVER_CONN_PACKET_LOSS: &str = "naia_server_conn_packet_loss";
pub const SERVER_CONN_KBPS_SENT:   &str = "naia_server_conn_kbps_sent";
pub const SERVER_CONN_KBPS_RECV:   &str = "naia_server_conn_kbps_recv";

// Client connection metrics (no label — one connection per process)
pub const CLIENT_CONN_RTT_MS:      &str = "naia_client_conn_rtt_ms";
pub const CLIENT_CONN_JITTER_MS:   &str = "naia_client_conn_jitter_ms";
pub const CLIENT_CONN_PACKET_LOSS: &str = "naia_client_conn_packet_loss";
pub const CLIENT_CONN_KBPS_SENT:   &str = "naia_client_conn_kbps_sent";
pub const CLIENT_CONN_KBPS_RECV:   &str = "naia_client_conn_kbps_recv";
