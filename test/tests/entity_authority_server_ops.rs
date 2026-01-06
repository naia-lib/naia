use naia_client::ClientConfig;
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.4: Delegated Authority: Server Priority Operations (give/take/release)
// ============================================================================

/// give_authority assigns to client and denies everyone else
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when server calls give_authority(A,E); then A observes Granted + AuthGranted(E) and B observes Denied + AuthDenied(E).
#[test]
fn give_authority_assigns_to_client_and_denies_everyone_else() {
    todo!()
}

/// take_authority forces server hold; all clients denied
///
/// Given delegated E with AuthNone (Available) in scope for A and B; when server calls take_authority(E); then both A and B observe Denied, and both emit AuthDenied(E) (from non-Denied to Denied).
#[test]
fn take_authority_forces_server_hold_all_clients_denied() {
    todo!()
}

/// Server-held authority is indistinguishable from "client is denied"
///
/// Given delegated E where server holds authority; then every in-scope client observes Denied (and cannot mutate), and no client observes Granted.
#[test]
fn server_held_authority_is_indistinguishable_from_client_is_denied() {
    todo!()
}

/// Server priority: take_authority overrides a client holder
///
/// Given delegated E where A currently holds authority (A Granted, B Denied); when server calls take_authority(E); then A transitions Granted→Denied emitting AuthLost(E) and AuthDenied(E); B remains Denied; all in-scope clients observe Denied.
#[test]
fn server_priority_take_authority_overrides_a_client_holder() {
    todo!()
}

/// Server priority: give_authority overrides current holder
///
/// Given delegated E where A currently holds authority; when server calls give_authority(B,E); then A transitions Granted→Denied emitting AuthLost(E) and AuthDenied(E); B transitions Denied/Available→Granted emitting AuthGranted(E); all other in-scope clients observe Denied.
#[test]
fn server_priority_give_authority_overrides_current_holder() {
    todo!()
}

/// Server release_authority clears holder; all clients Available
///
/// Given delegated E with any current holder (Server or Client); when server calls release_authority(E); then all in-scope clients observe Available; if a client previously held Granted it MUST emit AuthLost(E); any client previously Denied MUST observe Denied→Available.
#[test]
fn server_release_authority_clears_holder_all_clients_available() {
    todo!()
}

/// Former holder sees Granted→Available on server release
///
/// Given delegated E held by A; when server calls release_authority(E); then A emits AuthLost(E) and observes Available.
#[test]
fn former_holder_sees_granted_to_available_on_server_release() {
    todo!()
}

/// Server give_authority requires scope
///
/// Given delegated E where OutOfScope(A,E) holds; when server calls give_authority(A,E); then it returns ErrNotInScope and authority holder does not change.
#[test]
fn server_give_authority_requires_scope() {
    todo!()
}

