use naia_client::ClientConfig;
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientEntityAuthDeniedEvent, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.5: Authority + Scope Coupling
// ============================================================================

/// Authority releases when holder goes OutOfScope
///
/// Given delegated E where A holds authority and B observes Denied; when server removes E from A's scope (so A despawns E); then authority MUST release to None, and B MUST observe Denied→Available.
#[test]
fn authority_releases_when_holder_goes_out_of_scope() {
    todo!()
}

/// Authority releases when holder disconnects
///
/// Given delegated E where A holds authority and B is in scope; when A disconnects; then authority MUST release to None, and B MUST observe Available (or Denied→Available if previously denied), with E still alive and replicated per server policy.
#[test]
fn authority_releases_when_holder_disconnects() {
    todo!()
}

/// Re-entering scope yields correct current auth status
///
/// Given delegated E where A holds authority and B is Denied; when B goes out of scope then later comes back into scope; then B observes Denied (and emits AuthDenied only on transition into Denied, not on spawn if already Denied).
#[test]
fn re_entering_scope_yields_correct_current_auth_status() {
    todo!()
}

