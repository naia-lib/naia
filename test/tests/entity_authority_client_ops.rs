use naia_client::ClientConfig;
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.3: Delegated Authority: Client Operations (request/release)
// ============================================================================

/// request_authority(Available) grants to requester and denies everyone else
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when A calls request_authority(E); then A observes Granted + AuthGranted(E), and B observes Denied + AuthDenied(E).
#[test]
fn request_authority_available_grants_to_requester_and_denies_everyone_else() {
    todo!()
}

/// Non-holder cannot mutate delegated entity
///
/// Given delegated E where A is authority holder and B is Denied; when B attempts to mutate E; then mutation is ignored/rejected (no panics) and both clients converge on the authoritative state (from A/server).
#[test]
fn non_holder_cannot_mutate_delegated_entity() {
    todo!()
}

/// Holder can mutate delegated entity
///
/// Given delegated E where A is authority holder; when A mutates E; then server accepts and all in-scope clients observe the mutation.
#[test]
fn holder_can_mutate_delegated_entity() {
    todo!()
}

/// Denied client request_authority fails (ErrNotAvailable)
///
/// Given delegated E where A holds authority and B observes Denied; when B calls request_authority(E); then it returns ErrNotAvailable and authority holder remains A (no state/events change).
#[test]
fn denied_client_request_authority_fails_err_not_available() {
    todo!()
}

/// Holder release_authority transitions everyone to Available
///
/// Given delegated E where A holds authority and B observes Denied; when A calls release_authority(E); then A emits AuthLost(E) and both A and B observe Available (explicit Denied→Available for B).
#[test]
fn holder_release_authority_transitions_everyone_to_available() {
    todo!()
}

/// release_authority when not holder fails (ErrNotHolder)
///
/// Given delegated E where A holds authority and B observes Denied; when B calls release_authority(E); then it returns ErrNotHolder and nothing changes.
#[test]
fn release_authority_when_not_holder_fails_err_not_holder() {
    todo!()
}

