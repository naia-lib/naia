use naia_client::{ClientConfig, ReplicationConfig};
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.6: Client-Owned Delegation Migration + Event Correctness
// ============================================================================

/// Cannot delegate client-owned Unpublished (ErrNotPublished)
///
/// Given client-owned Unpublished E owned by A; when server (or A) attempts to delegate E; then operation fails with ErrNotPublished and E remains client-owned Unpublished.
#[test]
fn cannot_delegate_client_owned_unpublished_err_not_published() {
    todo!()
}

/// Delegating client-owned Published migrates identity without despawn+spawn
///
/// Given client-owned Published E owned by A and in scope for A and B; when server (or A) delegates E; then E remains the same identity on clients (continuity), and becomes server-owned delegated.
#[test]
fn delegating_client_owned_published_migrates_identity_without_despawn_spawn() {
    todo!()
}

/// Migration assigns initial authority to owner if owner in scope
///
/// Given client-owned Published E owned by A and InScope(A,E); when E is delegated (migrates); then resulting delegated E has holder Client(A): A observes Granted + AuthGranted(E), and every other in-scope client observes Denied + AuthDenied(E).
#[test]
fn migration_assigns_initial_authority_to_owner_if_owner_in_scope() {
    todo!()
}

/// Migration yields no holder if owner out of scope
///
/// Given client-owned Published E owned by A but OutOfScope(A,E) at migration moment; when E is delegated (migrates); then resulting delegated E has AuthNone and every in-scope client observes Available (no initial holder).
#[test]
fn migration_yields_no_holder_if_owner_out_of_scope() {
    todo!()
}

/// After migration, writes follow delegated rules
///
/// Given migrated delegated E; when owner A is not the authority holder; then A's mutations are ignored/rejected; when A later acquires authority (Available→Granted) then A's mutations are accepted.
#[test]
fn after_migration_writes_follow_delegated_rules() {
    todo!()
}

/// AuthGranted emitted exactly once on Available→Granted
///
/// Given delegated E Available for A; when A becomes holder (via request_authority or give_authority); then exactly one AuthGranted(E) is emitted to A for that transition.
#[test]
fn auth_granted_emitted_exactly_once_on_available_to_granted() {
    todo!()
}

/// AuthDenied emitted exactly once per transition into Denied
///
/// Given delegated E where a client C transitions into Denied (from Available or Granted); then exactly one AuthDenied(E) is emitted for that transition.
#[test]
fn auth_denied_emitted_exactly_once_per_transition_into_denied() {
    todo!()
}

/// AuthLost emitted exactly once per transition out of Granted
///
/// Given delegated E where client A transitions from Granted to (Denied or Available); then exactly one AuthLost(E) is emitted for that transition.
#[test]
fn auth_lost_emitted_exactly_once_per_transition_out_of_granted() {
    todo!()
}

/// No auth events for non-delegated entities, ever
///
/// Given any non-delegated entity (server-owned undelegated or any client-owned); then AuthGranted/AuthDenied/AuthLost MUST NOT be emitted regardless of scope/mutations.
#[test]
fn no_auth_events_for_non_delegated_entities_ever() {
    todo!()
}

/// Duplicate SetAuthority does not emit duplicate events
///
/// Given delegated E in a stable status S for client C; when server re-sends SetAuthority(S) (same status); then C emits no additional auth events and status remains S.
#[test]
fn duplicate_set_authority_does_not_emit_duplicate_events() {
    todo!()
}

