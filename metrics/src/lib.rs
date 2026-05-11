//! Observability emission layer for naia game networking.
//!
//! Emits network health data — RTT, jitter, packet loss, bandwidth — via the
//! [`metrics`] crate facade. Install any compatible exporter at startup and
//! naia's data flows to your monitoring backend automatically.
//!
//! # Non-Bevy usage
//!
//! Call once per tick after `server.send_all_packets()`:
//!
//! ```rust,ignore
//! naia_metrics::emit_server_aggregates(
//!     server.user_count(),
//!     server.entity_count(),
//!     server.room_count(),
//! );
//! for user_key in server.user_keys() {
//!     if let Some(stats) = server.connection_stats(&user_key) {
//!         naia_metrics::emit_server_connection_stats(&stats, user_key.to_u64());
//!     }
//! }
//! ```
//!
//! For Bevy apps, use [`naia-bevy-metrics`] instead — it handles emission
//! automatically via a plugin.

pub mod names;
mod server;
mod client;

pub use server::{emit_server_aggregates, emit_server_connection_stats};
pub use client::emit_client_connection_stats;
