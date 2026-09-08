#[cfg(debug_assertions)]
use naia_socket_shared::LinkConditionerConfig;
use naia_socket_shared::SocketConfig;

pub const PING_MSG: &str = "PING";
pub const PONG_MSG: &str = "PONG";

/// The protocol fingerprint these two demos agree on.
///
/// # Why this is a literal, and only here
///
/// A real application never writes one of these down. `naia-shared` computes
/// the fingerprint from the protocol's registered channels, messages,
/// components and resources, and `naia-client` / `naia-server` pass that
/// computed value into the socket layer for you -- there is no application-level
/// choice to make.
///
/// These demos are the one exception: they drive `naia-client-socket` and
/// `naia-server-socket` *directly*, below the layer that owns a `Protocol`, so
/// there is no registry here to derive anything from. The socket API requires
/// a fingerprint rather than accepting its absence, so the two demo halves
/// share one arbitrary constant.
///
/// # This cannot become an ungated production entry point
///
/// It is only ever an argument value. Every socket entry point --
/// `Socket::listen`, `Socket::listen_with_auth`, and all four client
/// `connect*` methods -- takes the fingerprint as a *required* parameter, with
/// no default, no `Option`, and no "skip the check" variant. Passing this
/// constant therefore configures a server that refuses everyone who does not
/// present exactly this value; it does not open a path that skips the
/// comparison. An application linking `naia-server` gets its own computed
/// fingerprint and never sees this constant at all.
pub const DEMO_PROTOCOL_ID: &str = "0000000000000000000000000000d000";

pub fn shared_config() -> SocketConfig {
    // The link conditioner simulates latency, jitter and packet loss. That is
    // what you want while developing, and almost never what you want in a
    // shipped build -- so gate it on the build profile rather than remembering
    // to delete it. Naia deliberately leaves the choice to the app (see
    // naia-lib/naia#65); this is just the pattern.
    #[cfg(debug_assertions)]
    let link_condition = Some(LinkConditionerConfig::average_condition());
    #[cfg(not(debug_assertions))]
    let link_condition = None;

    SocketConfig::new(link_condition, None)
}
