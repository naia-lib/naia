use naia_client::{ClientConfig, ReplicationConfig};
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientKey, Position, Scenario};
use test_helpers::test_client_config;

mod test_helpers;
use test_helpers::client_connect;

// ============================================================================
// Domain 4.1: Client-Owned Entities (Unpublished vs Published)
// ============================================================================

/// Client-owned (Unpublished) is visible only to owner
///
/// Given client A owns client-owned entity E in **Unpublished** state; when E exists; then A can see E, server can see E, and every non-owner client B MUST NOT have E in scope (E absent in B's world).
#[test]
fn client_owned_unpublished_is_visible_only_to_owner() {
    todo!()
}

/// Client-owned (Unpublished) replication is owner→server only
///
/// Given client-owned Unpublished E owned by A; when A mutates E; then server reflects the mutation; and any non-owner client B never observes E (no visibility, no replication to B).
#[test]
fn client_owned_unpublished_replication_is_owner_to_server_only() {
    todo!()
}

/// Client-owned (Published) may be scoped to non-owners
///
/// Given client-owned Published E owned by A; when server includes E in B's scope; then B observes E (E appears in B's world) with correct replicated state.
#[test]
fn client_owned_published_may_be_scoped_to_non_owners() {
    todo!()
}

/// Client-owned (Published) rejects non-owner mutations
///
/// Given client-owned Published E owned by A and in scope for B; when B attempts to mutate E; then server ignores/rejects B's mutation and authoritative state remains driven by A (and/or server replication), with no panics.
#[test]
fn client_owned_published_rejects_non_owner_mutations() {
    todo!()
}

/// Client-owned (Published) accepts owner mutations and propagates
///
/// Given client-owned Published E owned by A and in scope for B; when A mutates E; then server accepts and both A and B observe the updated state.
#[test]
fn client_owned_published_accepts_owner_mutations_and_propagates() {
    todo!()
}

/// Publish toggle: Published → Unpublished forcibly despawns for non-owners
///
/// Given client-owned Published E owned by A and currently in scope for B; when E becomes Unpublished (by server or owner A); then B MUST lose E from its world (OutOfScope), while A and server retain E.
#[test]
fn publish_toggle_published_to_unpublished_forcibly_despawns_for_non_owners() {
    todo!()
}

/// Publish toggle: Unpublished → Published enables scoping to non-owners
///
/// Given client-owned Unpublished E owned by A; when E becomes Published; then server can include E in B's scope and B observes E normally.
#[test]
fn publish_toggle_unpublished_to_published_enables_scoping_to_non_owners() {
    todo!()
}

/// Client-owned entities emit NO authority events
///
/// Given client-owned E (Published or Unpublished); when any replication and mutations occur; then clients MUST observe **no** AuthGranted/AuthDenied/AuthLost events for E.
#[test]
fn client_owned_entities_emit_no_authority_events() {
    todo!()
}

