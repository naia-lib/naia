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
//!     .add_plugins(NaiaServerPlugin::new(server_config(), protocol()))
//!     .add_plugins(NaiaServerMetricsPlugin)
//!     .run();
//! ```
//!
//! # Features
//!
//! Enable `server` for [`NaiaServerMetricsPlugin`]; `client` for
//! [`NaiaClientMetricsPlugin`]. Both can be enabled simultaneously for
//! listen-server setups.

#[cfg(feature = "server")]
mod server_plugin;
#[cfg(feature = "client")]
mod client_plugin;

#[cfg(feature = "server")]
pub use server_plugin::NaiaServerMetricsPlugin;
#[cfg(feature = "client")]
pub use client_plugin::NaiaClientMetricsPlugin;

#[cfg(feature = "client")]
pub use naia_bevy_client::DefaultClientTag;
#[cfg(feature = "client")]
pub type DefaultClientMetricsPlugin = NaiaClientMetricsPlugin<DefaultClientTag>;
