/// REGRESSION TEST FOR BUG #4: Invalid authority transition panic
///
/// THE BUG: AuthChannel didn't allow idempotent Available → Available transition.
/// When SetAuthority(Available) was called on an entity already in Available state,
/// it panicked with "Invalid authority transition from Available to Available".
///
/// ROOT CAUSE: The validate_command function only allowed state-changing transitions,
/// not idempotent ones (same state → same state).
///
/// THE SYMPTOM: Server panicked when resetting authority:
/// "thread 'main' panicked at shared/src/world/sync/auth_channel.rs:94:25:
///  Invalid authority transition from Available to Available"
///
/// THE FIX: Added (Available, Available) as a valid idempotent transition.
///
/// This test would have caught the bug if it existed before production.
use naia_shared::{
    BigMapKey, EntityAuthStatus, EntityCommand, GlobalEntity, HostEntityChannel, HostType,
};

/// Test that Available → Available transition is allowed (idempotent)
#[test]
fn bug_04_available_to_available_allowed() {
    let global_entity = GlobalEntity::from_u64(4001);

    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Enable delegation (sets authority to Available)
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Set authority to Available again (idempotent operation)
    // Before Bug #4 fix, this would panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::SetAuthority(
            Some(2),
            global_entity,
            EntityAuthStatus::Available,
        ));
    }));

    assert!(
        result.is_ok(),
        "BUG #4: Setting authority to Available when already Available should succeed (idempotent). \
         Before fix, this panicked with 'Invalid authority transition from Available to Available'"
    );
}

/// Test all valid authority transitions
#[test]
fn bug_04_all_valid_authority_transitions() {
    let global_entity = GlobalEntity::from_u64(4002);

    let mut host_channel = HostEntityChannel::new(HostType::Server);
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Available → Granted (valid)
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(2),
        global_entity,
        EntityAuthStatus::Granted,
    ));

    // Granted → Available (valid)
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(3),
        global_entity,
        EntityAuthStatus::Available,
    ));

    // Available → Denied (valid)
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(4),
        global_entity,
        EntityAuthStatus::Denied,
    ));

    // Denied → Available (valid)
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(5),
        global_entity,
        EntityAuthStatus::Available,
    ));

    // Available → Available (idempotent - Bug #4 fix)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::SetAuthority(
            Some(6),
            global_entity,
            EntityAuthStatus::Available,
        ));
    }));

    assert!(result.is_ok(), "All valid transitions should succeed");
}

/// Test that idempotent transitions work for other authority operations
#[test]
fn bug_04_idempotent_operations_safe() {
    let global_entity = GlobalEntity::from_u64(4003);

    let mut host_channel = HostEntityChannel::new(HostType::Server);
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Multiple ReleaseAuthority calls (should be safe)
    host_channel.send_command(EntityCommand::ReleaseAuthority(Some(2), global_entity));
    host_channel.send_command(EntityCommand::ReleaseAuthority(Some(3), global_entity));

    // Both should succeed (ReleaseAuthority sets to Available)
    let commands = host_channel.extract_outgoing_commands();
    assert!(commands.len() >= 3, "Idempotent operations should not fail");
}

/// Test authority state machine invariants
#[test]
fn bug_04_authority_state_machine_invariants() {
    let global_entity = GlobalEntity::from_u64(4004);

    let mut host_channel = HostEntityChannel::new(HostType::Server);
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Cycle through states multiple times
    for i in 0..3 {
        let base = (i * 3) + 2;

        host_channel.send_command(EntityCommand::SetAuthority(
            Some(base),
            global_entity,
            EntityAuthStatus::Granted,
        ));

        host_channel.send_command(EntityCommand::SetAuthority(
            Some(base + 1),
            global_entity,
            EntityAuthStatus::Available,
        ));

        host_channel.send_command(EntityCommand::SetAuthority(
            Some(base + 2),
            global_entity,
            EntityAuthStatus::Available, // Idempotent
        ));
    }

    let commands = host_channel.extract_outgoing_commands();
    assert!(
        commands.len() >= 10,
        "State machine should handle cycles with idempotent transitions"
    );
}
