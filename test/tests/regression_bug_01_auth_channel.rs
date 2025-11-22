/// REGRESSION TEST FOR BUG #1: AuthChannel command validation panic
///
/// THE BUG: AuthChannel::validate_command() didn't handle MigrateResponse, RequestAuthority,
/// EnableDelegationResponse, and Noop command types. When these commands were sent through
/// the channel, it panicked with "Unsupported command type".
///
/// ROOT CAUSE: The validate_command function had a catch-all panic for unhandled command types,
/// but several valid command types were missing from the match statement.
///
/// THE SYMPTOM: Server panicked when trying to send MigrateResponse during delegation:
/// "thread 'main' panicked at shared/src/world/sync/auth_channel.rs:101:17:
///  Unsupported command type for AuthChannelSender: MigrateResponse"
///
/// This test would have caught the bug if it existed before production.
use naia_shared::{
    BigMapKey, ComponentKind, EntityAuthStatus, EntityCommand, GlobalEntity, HostEntity,
    HostEntityChannel, HostType, RemoteEntity,
};
use std::any::TypeId;

fn component_kind<T: 'static>() -> ComponentKind {
    ComponentKind::from(TypeId::of::<T>())
}

#[derive(Debug, Clone, Copy)]
struct DummyComponent;

/// Test that MigrateResponse can be sent through AuthChannel
#[test]
fn bug_01_migrate_response_validation() {
    let global_entity = GlobalEntity::from_u64(10001);
    let old_remote_entity = RemoteEntity::new(99);
    let new_host_entity = HostEntity::new(100);

    // Server-side HostEntityChannel (Published → Delegated → MigrateResponse)
    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Publish is automatic for Server
    // Enable delegation
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // THE CRITICAL TEST: Send MigrateResponse
    // Before Bug #1 fix, this would panic with "Unsupported command type"
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::MigrateResponse(
            Some(2),
            global_entity,
            old_remote_entity,
            new_host_entity,
        ));
    }));

    assert!(
        result.is_ok(),
        "BUG #1: MigrateResponse should be accepted by AuthChannel. \
         Before fix, this panicked with 'Unsupported command type for AuthChannelSender: MigrateResponse'"
    );
}

/// Test that RequestAuthority can be sent through AuthChannel
#[test]
fn bug_01_request_authority_validation() {
    let global_entity = GlobalEntity::from_u64(10002);

    // Client-side HostEntityChannel (Unpublished → Published → Delegated → RequestAuthority)
    let mut host_channel = HostEntityChannel::new(HostType::Client);

    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Send RequestAuthority - this was also missing from validate_command
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::RequestAuthority(Some(3), global_entity));
    }));

    assert!(
        result.is_ok(),
        "RequestAuthority should be accepted by AuthChannel"
    );
}

/// Test that EnableDelegationResponse can be sent through AuthChannel
#[test]
fn bug_01_enable_delegation_response_validation() {
    let global_entity = GlobalEntity::from_u64(10003);

    let mut host_channel = HostEntityChannel::new(HostType::Client);
    host_channel.send_command(EntityCommand::Publish(Some(1), global_entity));
    host_channel.send_command(EntityCommand::EnableDelegation(Some(2), global_entity));

    // Send EnableDelegationResponse
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_channel.send_command(EntityCommand::EnableDelegationResponse(
            Some(3),
            global_entity,
        ));
    }));

    assert!(
        result.is_ok(),
        "EnableDelegationResponse should be accepted by AuthChannel"
    );
}

/// Test complete migration command sequence through AuthChannel
#[test]
fn bug_01_complete_migration_command_sequence() {
    let global_entity = GlobalEntity::from_u64(10004);
    let old_remote_entity = RemoteEntity::new(199);
    let new_host_entity = HostEntity::new(200);

    let mut host_channel = HostEntityChannel::new(HostType::Server);

    // Sequence: EnableDelegation → MigrateResponse
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));
    host_channel.send_command(EntityCommand::MigrateResponse(
        Some(2),
        global_entity,
        old_remote_entity,
        new_host_entity,
    ));

    let commands = host_channel.extract_outgoing_commands();
    assert!(
        commands.len() >= 2,
        "Both commands should be buffered successfully"
    );

    // Verify MigrateResponse is in the commands
    let has_migrate_response = commands
        .iter()
        .any(|cmd| matches!(cmd, EntityCommand::MigrateResponse(_, _, _, _)));
    assert!(
        has_migrate_response,
        "MigrateResponse should be in outgoing commands"
    );
}

/// Test that SetAuthority with all transitions works
#[test]
fn bug_01_set_authority_all_transitions() {
    let global_entity = GlobalEntity::from_u64(10005);

    let mut host_channel = HostEntityChannel::new(HostType::Server);
    host_channel.send_command(EntityCommand::EnableDelegation(Some(1), global_entity));

    // Test various SetAuthority transitions
    host_channel.send_command(EntityCommand::SetAuthority(
        Some(2),
        global_entity,
        EntityAuthStatus::Granted,
    ));

    host_channel.send_command(EntityCommand::SetAuthority(
        Some(3),
        global_entity,
        EntityAuthStatus::Available,
    ));

    host_channel.send_command(EntityCommand::SetAuthority(
        Some(4),
        global_entity,
        EntityAuthStatus::Denied,
    ));

    let commands = host_channel.extract_outgoing_commands();
    assert!(
        commands.len() >= 4,
        "All SetAuthority commands should succeed"
    );
}
