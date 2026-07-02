//! Bevy plugins for naia game networking metrics.
//!
//! Add [`NaiaServerMetricsPlugin`] to your Bevy [`App`] and naia's network
//! health data is emitted automatically each tick via the [`metrics`] crate
//! facade.
//!
//! # Setup
//!
//! ```rust,ignore
//! // 1. Install a metrics exporter at startup (user's choice of backend):
//! //    e.g. metrics_exporter_prometheus, metrics_exporter_statsd, etc.
//!
//! // 2. Add the plugin:
//! App::new()
//!     .add_plugins(NaiaServerPlugin::new(ServerPluginConfig::new(
//!         server_config(),
//!         protocol(),
//!         Topology::Standalone(DriveShape::Resident),
//!     )))
//!     .add_plugins(NaiaServerMetricsPlugin)
//!     .run();
//! ```
//!
//! # Features
//!
//! Enable `server` for [`NaiaServerMetricsPlugin`]; `client` for
//! [`NaiaClientMetricsPlugin`]. Both can be enabled simultaneously for
//! listen-server setups.

cfg_if::cfg_if! {
    if #[cfg(feature = "server")] {
        mod server_plugin;
        pub use server_plugin::NaiaServerMetricsPlugin;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "client")] {
        mod client_plugin;
        pub use client_plugin::NaiaClientMetricsPlugin;
        pub use naia_bevy_client::DefaultClientTag;
        pub type DefaultClientMetricsPlugin = NaiaClientMetricsPlugin<DefaultClientTag>;
    }
}
