# Entity Migration Implementation Plan

**Status:** Complete specification - Ready for implementation  
**Based on:** `MIGRATION_FEATURE_SPEC.md`  
**Objective:** Implement atomic, single-tick entity migration with zero data loss during delegation

---

## Overview

This plan details all code changes required to implement entity migration from RemoteEntity to HostEntity (server-side) and HostEntity to RemoteEntity (client-side). Changes are organized by file and must be executed in the specified order to maintain compilation.

---

## Phase 1: Core Channel Infrastructure

Add fundamental methods to entity channels and engines that will be used by migration logic.

### File 1: `shared/src/world/sync/remote_component_channel.rs`

**Purpose:** Expose component state for extraction during migration

**Changes:**

1. **Add public accessor for `inserted` state**
   ```rust
   pub(crate) fn is_inserted(&self) -> bool {
       self.inserted
   }
   ```

2. **Add force-drain method** (processes all buffered operations regardless of FSM state)
   ```rust
   pub(crate) fn force_drain_buffers(&mut self, entity_state: EntityChannelState) {
       // While there are buffered messages:
       //   - Pop from buffered_messages
       //   - Apply the insert/remove regardless of current `inserted` state
       //   - Update `inserted` flag and `last_epoch_id`
       //   - Push operation to incoming_messages
       // This ensures no buffered operations are lost during migration
       // Accept temporary inconsistency for the sake of zero data loss
   }
   ```

**Rationale:** These methods allow RemoteEntityChannel to extract the true component state and resolve all pending operations before migration.

---

### File 2: `shared/src/world/sync/remote_entity_channel.rs`

**Purpose:** Add extraction, force-draining, and state inspection for migration

**Changes:**

1. **Add state accessor**
   ```rust
   pub(crate) fn get_state(&self) -> EntityChannelState {
       self.state
   }
   ```

2. **Add component extraction method**
   ```rust
   pub(crate) fn extract_inserted_component_kinds(&self) -> HashSet<ComponentKind> {
       // Iterate through component_channels
       // Filter where is_inserted() == true
       // Collect ComponentKind keys into HashSet
       // Returns only components that are actually inserted
   }
   ```

3. **Add force-drain method for all buffers**
   ```rust
   pub(crate) fn force_drain_all_buffers(&mut self) {
       // Force-drain entity-level buffered_messages
       // For each message in buffered_messages (regardless of FSM state):
       //   - Process it according to type (Publish, Unpublish, etc.)
       //   - Update auth_channel or component_channels
       //   - Move to incoming_messages
       
       // For each component_channel:
       //   - Call component_channel.force_drain_buffers(self.state)
       
       // After this, all buffers are empty and state is resolved
   }
   ```

**Rationale:** Migration needs to extract the true state (which components exist) and ensure no operations are pending.

---

### File 3: `shared/src/world/sync/host_entity_channel.rs`

**Purpose:** Add constructor that accepts pre-populated component state and command extraction

**Changes:**

1. **Add new constructor with components**
   ```rust
   pub(crate) fn new_with_components(
       host_type: HostType,
       component_kinds: HashSet<ComponentKind>
   ) -> Self {
       Self {
           component_channels: component_kinds, // Pre-populate with existing components
           auth_channel: AuthChannel::new(host_type),
           buffered_messages: OrderedIds::new(),
           incoming_messages: Vec::new(),
           outgoing_commands: Vec::new(),
       }
   }
   ```

2. **Add command extraction method**
   ```rust
   pub(crate) fn extract_outgoing_commands(&mut self) -> Vec<EntityCommand> {
       // Return std::mem::take(&mut self.outgoing_commands)
       // Used by client to buffer commands during migration
   }
   ```

**Rationale:** When creating a HostEntityChannel during migration, we need to initialize it with the component state from the old RemoteEntityChannel.

---

### File 4: `shared/src/world/sync/remote_engine.rs`

**Purpose:** Add method to remove and extract a RemoteEntityChannel

**Changes:**

1. **Add remove_entity_channel method**
   ```rust
   pub(crate) fn remove_entity_channel(&mut self, entity: &E) -> RemoteEntityChannel {
       // Remove entity_channel from entity_channels HashMap
       // Return the extracted channel
       // Panic if entity doesn't exist
       self.entity_channels.remove(entity)
           .expect("Cannot remove entity channel that doesn't exist")
   }
   ```

**Rationale:** Migration needs to extract the channel to transfer its state to the new engine.

---

### File 5: `shared/src/world/sync/host_engine.rs`

**Purpose:** Add method to remove and extract a HostEntityChannel, and method to insert channel with pre-populated state

**Changes:**

1. **Add remove_entity_channel method**
   ```rust
   pub(crate) fn remove_entity_channel(&mut self, entity: &HostEntity) -> HostEntityChannel {
       self.entity_channels.remove(entity)
           .expect("Cannot remove entity channel that doesn't exist")
   }
   ```

2. **Add insert_entity_channel method**
   ```rust
   pub(crate) fn insert_entity_channel(
       &mut self,
       entity: HostEntity,
       channel: HostEntityChannel
   ) {
       // Insert the pre-constructed channel into entity_channels
       // Used during migration to install the new channel
       if self.entity_channels.contains_key(&entity) {
           panic!("Cannot insert entity channel that already exists");
       }
       self.entity_channels.insert(entity, channel);
   }
   ```

**Rationale:** Client-side migration needs to extract HostEntityChannel and insert a new RemoteEntityChannel. Server provides symmetry.

---

## Phase 2: Entity Redirect System

Implement the redirect table to handle in-flight messages after migration.

### File 6: `shared/src/world/local/local_entity_record.rs`

**Purpose:** Understand the current structure (no changes needed, just for reference)

**No changes required** - This file defines `LocalEntityRecord` which stores either `Host(u16)` or `Remote(u16)`. The redirect system will work at a higher level.

---

### File 7: `shared/src/world/local/local_entity_map.rs`

**Purpose:** Track entity redirects for handling in-flight messages

**Changes:**

1. **Add redirect table field to struct**
   ```rust
   pub struct LocalEntityMap {
       host_type: HostType,
       global_to_local: HashMap<GlobalEntity, LocalEntityRecord>,
       host_to_global: HashMap<HostEntity, GlobalEntity>,
       remote_to_global: HashMap<RemoteEntity, GlobalEntity>,
       
       // NEW: Redirect table for migrated entities
       entity_redirects: HashMap<OwnedLocalEntity, OwnedLocalEntity>, // old -> new
   }
   ```

2. **Initialize redirect table in `new()`**
   ```rust
   pub fn new(host_type: HostType) -> Self {
       Self {
           // ... existing fields ...
           entity_redirects: HashMap::new(),
       }
   }
   ```

3. **Add method to install redirect**
   ```rust
   pub(crate) fn install_entity_redirect(
       &mut self,
       old_entity: OwnedLocalEntity,
       new_entity: OwnedLocalEntity
   ) {
       // Store mapping: old_entity -> new_entity
       // This will be checked when reading/writing entity references
       self.entity_redirects.insert(old_entity, new_entity);
   }
   ```

4. **Add method to apply redirect**
   ```rust
   pub(crate) fn apply_entity_redirect(
       &self,
       entity: &OwnedLocalEntity
   ) -> OwnedLocalEntity {
       // Check if entity has a redirect
       // If yes, return the new entity
       // If no, return the original entity
       self.entity_redirects.get(entity).copied().unwrap_or(*entity)
   }
   ```

5. **Add cleanup method (called periodically)**
   ```rust
   pub(crate) fn cleanup_expired_redirects(&mut self, now: &Instant, ttl: Duration) {
       // This would require storing timestamps with redirects
       // For simplicity, redirects are kept for COMMAND_RECORD_TTL (60 seconds)
       // and cleaned up when sent_command_packets are cleaned up
       // Implementation detail: add timestamp to redirect entry
   }
   ```

**Rationale:** When serializing entity commands, we check the redirect table and use the new entity ID. When deserializing, we also check for redirects.

---

### File 8: `shared/src/world/local/local_world_manager.rs`

**Purpose:** Add high-level redirect management and sent_command_packets updating

**Changes:**

1. **Add helper method to update sent_command_packets entity references**
   ```rust
   fn update_sent_command_entity_refs(
       &mut self,
       global_entity: &GlobalEntity,
       old_entity: OwnedLocalEntity,
       new_entity: OwnedLocalEntity
   ) {
       // Iterate through sent_command_packets
       // For each packet's command list:
       //   For each (command_id, EntityMessage<OwnedLocalEntity>):
       //     If message references old_entity:
       //       Update message to reference new_entity instead
       // This ensures retransmissions use the correct entity ID
       
       // Pseudocode:
       // for (packet_index, (timestamp, commands)) in self.sent_command_packets.iter_mut() {
       //     for (command_id, message) in commands.iter_mut() {
       //         if message.entity() == Some(old_entity) {
       //             *message = message.with_entity(new_entity);
       //         }
       //     }
       // }
   }
   ```

2. **Add method to force-drain entity buffers**
   ```rust
   fn force_drain_entity_buffers(&mut self, global_entity: &GlobalEntity) {
       // Determine if entity is Host or Remote
       // Call appropriate engine's force-drain method
       // This resolves all pending operations before migration
       
       let Ok(local_entity) = self.entity_map.global_entity_to_owned_entity(global_entity) else {
           panic!("Entity does not exist in local entity map");
       };
       
       match local_entity {
           OwnedLocalEntity::Host(host_entity) => {
               // Host channels have minimal buffering, typically empty
               // Drain auth buffers if any
           }
           OwnedLocalEntity::Remote(remote_entity) => {
               // Get remote_entity_channel from remote engine
               // Call channel.force_drain_all_buffers()
           }
       }
   }
   ```

3. **Fix the broken `migrate_entity_remote_to_host` method** (lines 117-136)

   **Current broken code:**
   ```rust
   pub fn migrate_entity_remote_to_host(
       &mut self,
       global_entity: &GlobalEntity,
   ) -> HostEntity {
       let Some(local_entity_record) = self.entity_map.remove_by_global_entity(global_entity) else {
           panic!("...");
       };
       if !local_entity_record.is_remote_owned() {
           panic!("...");
       }
       let old_remote_entity = local_entity_record.remote_entity();

       let new_host_entity = self.host.host_generate_entity();
       self.entity_map.insert_with_host_entity(*global_entity, new_host_entity);

       let remote_entity_channel = self.remote.remove_entity_channel(&old_remote_entity); // ERROR 1
       self.remote.despawn_entity(&mut self.entity_map, &remote_entity); // ERROR 2: typo
       new_host_entity
   }
   ```

   **New implementation:**
   ```rust
   pub fn migrate_entity_remote_to_host(
       &mut self,
       global_entity: &GlobalEntity,
   ) -> HostEntity {
       // Validate entity exists and is remote-owned
       let Some(local_entity_record) = self.entity_map.remove_by_global_entity(global_entity) else {
           panic!("Attempting to migrate entity which does not exist: {:?}", global_entity);
       };
       if !local_entity_record.is_remote_owned() {
           panic!("Attempting to migrate non-remote entity: {:?}", global_entity);
       }
       let old_remote_entity = local_entity_record.remote_entity();

       // Step 1: Force-drain all buffers in RemoteEntityChannel
       //   Get mutable reference to remote_entity_channel
       //   Call force_drain_all_buffers()
       //   This resolves all pending operations before migration
       self.remote.force_drain_entity_buffers(&old_remote_entity);

       // Step 2: Extract component state from RemoteEntityChannel
       let component_kinds = self.remote.extract_component_kinds(&old_remote_entity);

       // Step 3: Remove RemoteEntityChannel from RemoteEngine
       let _old_remote_channel = self.remote.remove_entity_channel(&old_remote_entity);
       // (We extract but don't need to keep it - state is already extracted)

       // Step 4: Generate new HostEntity ID
       let new_host_entity = self.host.host_generate_entity();

       // Step 5: Create new HostEntityChannel with extracted component state
       //   Use HostEntityChannel::new_with_components(host_type, component_kinds)
       let new_host_channel = HostEntityChannel::new_with_components(
           self.host.host_type,
           component_kinds
       );

       // Step 6: Insert new HostEntityChannel into HostEngine
       self.host.insert_entity_channel(new_host_entity, new_host_channel);

       // Step 7: Update LocalEntityMap with new mapping
       self.entity_map.insert_with_host_entity(*global_entity, new_host_entity);

       // Step 8: Install entity redirect in LocalEntityMap
       let old_entity = OwnedLocalEntity::Remote(old_remote_entity.value());
       let new_entity = OwnedLocalEntity::Host(new_host_entity.value());
       self.entity_map.install_entity_redirect(old_entity, new_entity);

       // Step 9: Update all references in sent_command_packets
       self.update_sent_command_entity_refs(global_entity, old_entity, new_entity);

       // Step 10: Clean up old remote entity from waitlist
       self.remote.despawn_entity(&mut self.entity_map, &old_remote_entity);

       new_host_entity
   }
   ```

**Rationale:** This is the core server-side migration function. It must be comprehensive and correct.

---

### File 9: `shared/src/world/remote/remote_world_manager.rs`

**Purpose:** Add helper methods for migration called by LocalWorldManager

**Changes:**

1. **Add force_drain_entity_buffers method**
   ```rust
   pub(crate) fn force_drain_entity_buffers(&mut self, remote_entity: &RemoteEntity) {
       // Get mutable reference to remote_entity_channel
       // Call channel.force_drain_all_buffers()
       let Some(channel) = self.remote_engine.get_world_mut().get_mut(remote_entity) else {
           panic!("Cannot force-drain non-existent entity");
       };
       channel.force_drain_all_buffers();
   }
   ```

2. **Add extract_component_kinds method**
   ```rust
   pub(crate) fn extract_component_kinds(&self, remote_entity: &RemoteEntity) -> HashSet<ComponentKind> {
       let Some(channel) = self.remote_engine.get_world().get(remote_entity) else {
           panic!("Cannot extract component kinds from non-existent entity");
       };
       channel.extract_inserted_component_kinds()
   }
   ```

3. **Add remove_entity_channel method**
   ```rust
   pub(crate) fn remove_entity_channel(&mut self, remote_entity: &RemoteEntity) -> RemoteEntityChannel {
       self.remote_engine.remove_entity_channel(remote_entity)
   }
   ```

4. **Fix `send_auth_command` to handle missing entity gracefully** (line 141 error)
   ```rust
   pub(crate) fn send_auth_command(
       &mut self,
       converter: &dyn LocalEntityAndGlobalEntityConverter,
       command: EntityCommand
   ) {
       let global_entity = command.entity();
       
       // Check for entity redirect first
       let remote_entity_result = converter.global_entity_to_remote_entity(&global_entity);
       
       if remote_entity_result.is_err() {
           // Entity might have been migrated - this is expected during migration
           // Command will be discarded or handled by redirect system
           warn!("Cannot send auth command for entity {:?} - entity may have migrated", global_entity);
           return;
       }
       
       let remote_entity = remote_entity_result.unwrap();
       self.remote_engine.send_auth_command(remote_entity, command);
   }
   ```

**Rationale:** Provides clean interface for LocalWorldManager to interact with RemoteEngine during migration.

---

### File 10: `shared/src/world/host/host_world_manager.rs`

**Purpose:** Add helper methods for client-side migration

**Changes:**

1. **Add remove_entity_channel method**
   ```rust
   pub(crate) fn remove_entity_channel(&mut self, host_entity: &HostEntity) -> HostEntityChannel {
       self.host_engine.remove_entity_channel(host_entity)
   }
   ```

2. **Add insert_entity_channel method**
   ```rust
   pub(crate) fn insert_entity_channel(
       &mut self,
       remote_entity: RemoteEntity,
       channel: RemoteEntityChannel
   ) {
       // This is used on client side when migrating HostEntity -> RemoteEntity
       // We need to insert into the delivered_engine (which is a RemoteEngine<HostEntity>)
       // Actually, we need to think about this more carefully...
       
       // Client side has:
       // - host_engine: HostEngine (for client-owned non-delegated entities)
       // - delivered_engine: RemoteEngine<HostEntity> (tracks what server has received)
       
       // During client-side migration:
       // - Extract HostEntityChannel from host_engine
       // - Create RemoteEntityChannel with component state
       // - Insert into... wait, where?
       
       // Actually, client receives delegated entities as RemoteEntity in RemoteWorldManager
       // So client-side migration is handled in RemoteWorldManager, not here!
   }
   ```

**Note:** After analysis, client-side migration is actually handled differently - see Phase 3.

---

## Phase 3: Client-Side Migration

Client receives MigrateResponse and migrates HostEntity -> RemoteEntity.

### File 11: `client/src/client.rs`

**Purpose:** Implement client-side migration in MigrateResponse handler

**Current broken code (lines 1668-1684):**
```rust
EntityEvent::MigrateResponse(global_entity, _remote_entity) => {
    let world_entity = self
        .global_entity_map
        .global_entity_to_entity(&global_entity)
        .unwrap();
    self.entity_complete_delegation(world, &global_entity, &world_entity);

    self.global_world_manager
        .entity_update_authority(&global_entity, EntityAuthStatus::Granted);

    // self.add_redundant_remote_entity_to_host(&world_entity, remote_entity);

    todo!(); // <-- This is what we need to implement

    self.incoming_world_events.push_auth_grant(world_entity);
}
```

**New implementation:**
```rust
EntityEvent::MigrateResponse(global_entity, new_remote_entity) => {
    // Step 1: Get world entity
    let world_entity = self
        .global_entity_map
        .global_entity_to_entity(&global_entity)
        .unwrap();

    // Step 2: Get old HostEntity from LocalEntityMap
    let Some(connection) = &mut self.server_connection else {
        panic!("Received MigrateResponse without connection");
    };
    let old_host_entity = connection.base.world_manager
        .entity_converter()
        .global_entity_to_host_entity(&global_entity)
        .expect("Entity should exist as HostEntity before migration");

    // Step 3: Extract and buffer outgoing commands from HostEntityChannel
    //   Get mutable ref to host_entity_channel
    //   Call extract_outgoing_commands()
    //   Store commands for replay after migration
    let buffered_commands = connection.base.world_manager
        .extract_host_entity_commands(&global_entity);

    // Step 4: Force-drain any buffered messages (should be empty for Host)
    //   Host channels typically don't buffer incoming messages
    //   But drain for safety

    // Step 5: Extract component state from HostEntityChannel
    let component_kinds = connection.base.world_manager
        .extract_host_component_kinds(&global_entity);

    // Step 6: Remove HostEntityChannel from HostEngine
    //   This also removes from LocalEntityMap
    connection.base.world_manager
        .remove_host_entity(&global_entity);

    // Step 7: Create RemoteEntityChannel with extracted component state
    //   RemoteEntityChannel doesn't have a constructor with components
    //   We'll need to:
    //   a) Create new RemoteEntityChannel
    //   b) Spawn it (set state to Spawned)
    //   c) For each component_kind, insert RemoteComponentChannel with inserted=true

    // Step 8: Insert RemoteEntityChannel into RemoteEngine
    //   Use new_remote_entity ID from MigrateResponse
    connection.base.world_manager
        .insert_remote_entity(&global_entity, new_remote_entity, component_kinds);

    // Step 9: Install entity redirect in LocalEntityMap
    let old_entity = OwnedLocalEntity::Host(old_host_entity.value());
    let new_entity = OwnedLocalEntity::Remote(new_remote_entity.value());
    connection.base.world_manager
        .install_entity_redirect(old_entity, new_entity);

    // Step 10: Update sent_command_packets entity references
    connection.base.world_manager
        .update_sent_command_entity_refs(&global_entity, old_entity, new_entity);

    // Step 11: Re-validate and replay buffered commands
    //   Filter out invalid commands (Publish/Unpublish no longer valid)
    //   Re-queue valid commands to RemoteEntityChannel
    for command in buffered_commands {
        if command.is_valid_for_remote_entity() {
            connection.base.world_manager
                .replay_entity_command(&global_entity, command);
        }
    }

    // Step 12: Complete delegation in global world manager
    self.entity_complete_delegation(world, &global_entity, &world_entity);

    // Step 13: Update authority status
    self.global_world_manager
        .entity_update_authority(&global_entity, EntityAuthStatus::Granted);

    // Step 14: Emit AuthGrant event
    self.incoming_world_events.push_auth_grant(world_entity);
}
```

**Required helper methods in LocalWorldManager:**
```rust
// Add these to LocalWorldManager:

pub fn extract_host_entity_commands(&mut self, global_entity: &GlobalEntity) -> Vec<EntityCommand> {
    // Get host_entity from entity_map
    // Get host_entity_channel from host engine
    // Call extract_outgoing_commands()
}

pub fn extract_host_component_kinds(&self, global_entity: &GlobalEntity) -> HashSet<ComponentKind> {
    // Get host_entity from entity_map
    // Get host_entity_channel from host engine
    // Return component_channels clone
}

pub fn remove_host_entity(&mut self, global_entity: &GlobalEntity) {
    // Remove from entity_map
    // Remove from host engine
}

pub fn insert_remote_entity(
    &mut self,
    global_entity: &GlobalEntity,
    remote_entity: RemoteEntity,
    component_kinds: HashSet<ComponentKind>
) {
    // Insert into entity_map
    // Create RemoteEntityChannel
    // Set state to Spawned
    // For each component_kind, add RemoteComponentChannel with inserted=true
    // Insert into remote engine
}

pub fn install_entity_redirect(&mut self, old: OwnedLocalEntity, new: OwnedLocalEntity) {
    self.entity_map.install_entity_redirect(old, new);
}

pub fn update_sent_command_entity_refs(
    &mut self,
    global_entity: &GlobalEntity,
    old: OwnedLocalEntity,
    new: OwnedLocalEntity
) {
    // (Already defined in Phase 2, File 8)
}

pub fn replay_entity_command(&mut self, global_entity: &GlobalEntity, command: EntityCommand) {
    // Send command through appropriate channel (should be remote after migration)
}
```

**Rationale:** Client-side migration is symmetric to server-side but happens in response to MigrateResponse message.

---

## Phase 4: Serialization and Redirect Handling

Update world reader/writer to apply redirects when serializing/deserializing entity references.

### File 12: `shared/src/world/world_writer.rs`

**Purpose:** Apply entity redirects when writing EntityCommands

**Changes:**

1. **Modify entity command serialization to check redirects**

   In the function that serializes EntityCommand, before writing entity references:
   ```rust
   // When writing an EntityCommand<GlobalEntity> to wire format:
   
   // Convert GlobalEntity -> OwnedLocalEntity
   let mut local_entity = world_manager
       .entity_converter()
       .global_entity_to_owned_entity(global_entity)
       .unwrap();
   
   // NEW: Apply redirect if entity was migrated
   local_entity = world_manager
       .entity_converter()
       .apply_entity_redirect(&local_entity);
   
   // Then serialize local_entity to wire
   local_entity.ser(writer);
   ```

   This ensures that if an entity was migrated, we always write the NEW entity ID, even for messages that were queued before migration.

2. **Specific location:** Look for where MigrateResponse is serialized (already shows applying redirect in current code at line ~441 based on uncommitted changes)

**Rationale:** When retransmitting commands, use the new entity ID post-migration.

---

### File 13: `shared/src/world/world_reader.rs`

**Purpose:** Apply entity redirects when reading EntityMessages

**Changes:**

1. **Modify entity message deserialization to check redirects**

   In the function that deserializes EntityMessage:
   ```rust
   // After reading OwnedLocalEntity from wire:
   let mut local_entity = OwnedLocalEntity::de(reader)?;
   
   // NEW: Apply redirect if entity was migrated
   local_entity = world_manager
       .entity_converter()
       .apply_entity_redirect(&local_entity);
   
   // Then convert to GlobalEntity and process
   let global_entity = world_manager
       .entity_converter()
       .owned_entity_to_global_entity(&local_entity)
       .unwrap_or_else(|_| {
           // Entity doesn't exist - might be a late-arriving message for migrated entity
           // If redirect exists, it will have been applied above
           // If still doesn't exist, discard message
           warn!("Received message for unknown entity: {:?}", local_entity);
           return Err(SerdeErr::CannotDeserialize);
       });
   ```

**Rationale:** Handle messages that arrive after migration by applying the redirect before lookup.

---

## Phase 5: RemoteEngine Enhancements

Add methods needed by migration and force-draining.

### File 14: `shared/src/world/sync/remote_engine.rs`

**Purpose:** Add get_world_mut and force-drain support

**Changes:**

1. **Add mutable world accessor**
   ```rust
   pub(crate) fn get_world_mut(&mut self) -> &mut HashMap<E, RemoteEntityChannel> {
       &mut self.entity_channels
   }
   ```

2. **Add insert_entity_channel method**
   ```rust
   pub(crate) fn insert_entity_channel(&mut self, entity: E, channel: RemoteEntityChannel) {
       if self.entity_channels.contains_key(&entity) {
           panic!("Cannot insert entity channel that already exists");
       }
       self.entity_channels.insert(entity, channel);
   }
   ```

**Rationale:** Allows force-draining of specific entities and insertion of pre-constructed channels.

---

## Phase 6: Entity Command Validation

Add validation for commands to determine if they're valid after migration.

### File 15: `shared/src/world/entity_command.rs`

**Purpose:** Add helper to determine if command is valid for remote entities

**Changes:**

1. **Add validation method**
   ```rust
   impl EntityCommand {
       pub(crate) fn is_valid_for_remote_entity(&self) -> bool {
           // During client-side migration, some commands become invalid
           // Publish/Unpublish don't make sense for delegated entities
           // Delegation commands don't make sense post-delegation
           match self.get_type() {
               EntityMessageType::Publish | 
               EntityMessageType::Unpublish |
               EntityMessageType::EnableDelegation |
               EntityMessageType::DisableDelegation => false,
               
               EntityMessageType::InsertComponent |
               EntityMessageType::RemoveComponent |
               EntityMessageType::Despawn => true,
               
               _ => false,
           }
       }
   }
   ```

**Rationale:** Client needs to filter buffered commands when replaying after migration.

---

## Phase 7: Helper Methods and Plumbing

Add all the missing plumbing methods identified in earlier phases.

### File 16: `shared/src/world/local/local_world_manager.rs` (Additional methods)

**Purpose:** Add all helper methods required by client.rs

**Changes:**

Add all the helper methods defined in Phase 3, File 11:
- `extract_host_entity_commands()`
- `extract_host_component_kinds()`
- `remove_host_entity()`
- `insert_remote_entity()`
- `replay_entity_command()`

(Detailed pseudocode provided in Phase 3)

---

### File 17: `shared/src/world/remote/remote_world_manager.rs` (Additional methods)

**Purpose:** Support client-side migration into RemoteEngine

**Changes:**

1. **Add insert_entity_with_channel method**
   ```rust
   pub(crate) fn insert_entity_with_channel(
       &mut self,
       entity: RemoteEntity,
       channel: RemoteEntityChannel
   ) {
       self.remote_engine.insert_entity_channel(entity, channel);
   }
   ```

2. **Add method to create RemoteEntityChannel with component state**
   ```rust
   pub(crate) fn create_entity_channel_with_components(
       &self,
       component_kinds: HashSet<ComponentKind>
   ) -> RemoteEntityChannel {
       // Create new RemoteEntityChannel
       let mut channel = RemoteEntityChannel::new(self.host_type);
       
       // Set state to Spawned (entity already exists in world)
       // This requires adding a method to RemoteEntityChannel:
       // channel.set_spawned(message_id);
       
       // For each component_kind:
       //   Create RemoteComponentChannel
       //   Set inserted = true
       //   Insert into channel.component_channels
       
       // Return populated channel
   }
   ```

**Rationale:** Client needs to create a fully-initialized RemoteEntityChannel during migration.

---

### File 18: `shared/src/world/sync/remote_entity_channel.rs` (Additional methods)

**Purpose:** Support creating pre-spawned channels

**Changes:**

1. **Add method to set spawned state**
   ```rust
   pub(crate) fn set_spawned(&mut self, epoch_id: MessageIndex) {
       if self.state != EntityChannelState::Despawned {
           panic!("Can only set spawned on despawned entity");
       }
       self.state = EntityChannelState::Spawned;
       self.last_epoch_id = Some(epoch_id);
   }
   ```

2. **Add method to insert component channel with state**
   ```rust
   pub(crate) fn insert_component_channel_as_inserted(
       &mut self,
       component_kind: ComponentKind,
       epoch_id: MessageIndex
   ) {
       let mut comp_channel = RemoteComponentChannel::new();
       comp_channel.set_inserted(true, epoch_id);
       self.component_channels.insert(component_kind, comp_channel);
   }
   ```

**Rationale:** Client-side migration needs to create a RemoteEntityChannel that's already in Spawned state with components inserted.

---

## Phase 8: Server Delegation Handler Integration

Ensure server's enable_delegation_client_owned_entity properly triggers migration.

### File 19: `server/src/server/world_server.rs`

**Purpose:** Verify migration is properly called (code already looks correct)

**Changes:**

**Review existing code (lines 1560-1567):**
```rust
// Send EntityMigrateResponse action through EntityActionEvent system
let new_host_entity = connection
    .base
    .world_manager
    .migrate_entity_remote_to_host(global_entity);
connection
    .base
    .world_manager
    .host_send_migrate_response(global_entity, &new_host_entity);
```

**Verification:** This code is already correct! It calls our fixed `migrate_entity_remote_to_host` method and sends the MigrateResponse message. No changes needed here after Phase 2 fixes.

---

## Phase 9: Testing Infrastructure

Create test helpers to verify migration works correctly.

### File 20: `test/src/auth.rs` (or new file)

**Purpose:** Add unit tests for migration functions

**Changes:**

1. **Test server-side migration**
   ```rust
   #[test]
   fn test_migrate_entity_remote_to_host() {
       // Setup: Create LocalWorldManager with RemoteEntity
       // Add some components to RemoteEntityChannel
       // Buffer some operations
       
       // Execute: Call migrate_entity_remote_to_host
       
       // Verify:
       // - Entity exists as HostEntity
       // - Entity does not exist as RemoteEntity
       // - Component state preserved
       // - Redirect installed
       // - No buffered operations remain
   }
   ```

2. **Test client-side migration**
   ```rust
   #[test]
   fn test_client_migrate_response_handler() {
       // Setup: Create client with HostEntity
       // Add components
       // Queue some commands
       
       // Execute: Process MigrateResponse event
       
       // Verify:
       // - Entity exists as RemoteEntity
       // - Entity does not exist as HostEntity
       // - Component state preserved
       // - Commands replayed correctly
       // - Invalid commands filtered out
   }
   ```

3. **Test redirect system**
   ```rust
   #[test]
   fn test_entity_redirects() {
       // Setup: Install redirect old -> new
       
       // Execute: Serialize command with old entity
       
       // Verify: Wire format contains new entity ID
   }
   ```

4. **Test force-drain**
   ```rust
   #[test]
   fn test_force_drain_buffers() {
       // Setup: Create RemoteEntityChannel with buffered operations
       
       // Execute: Call force_drain_all_buffers
       
       // Verify: All operations processed, buffers empty
   }
   ```

**Rationale:** Comprehensive tests ensure migration works correctly in all scenarios.

---

## Implementation Order Summary

**Critical path (blocking compilation):**
1. Phase 1: Files 1-5 (Core channel infrastructure)
2. Phase 2: Files 6-10 (Redirect system and fix broken migration)
3. Phase 5: File 14 (RemoteEngine enhancements)
4. Phase 7: Files 16-18 (Helper methods)
5. Phase 3: File 11 (Client-side migration)

**Optimization and validation (non-blocking):**
6. Phase 4: Files 12-13 (Serialization redirect handling)
7. Phase 6: File 15 (Command validation)
8. Phase 8: File 19 (Server verification)
9. Phase 9: File 20 (Testing)

---

## Key Invariants to Maintain

1. **Single Source of Truth:** Entity exists in exactly one engine at any time
2. **Atomic Migration:** From external observer, migration is instantaneous
3. **Zero Data Loss:** All buffered operations and component state preserved
4. **Redirect Validity:** Redirects maintained for COMMAND_RECORD_TTL (60s)
5. **Component State Consistency:** Only insert ed components transferred

---

## Edge Cases Handled

1. **EC1 (Client commands during migration):** Buffered and replayed
2. **EC2 (In-flight messages):** Handled by redirect tables
3. **EC6 (Buffered channel messages):** Force-drained before migration
4. **EC7 (Component state):** Extracted and transferred precisely
5. **EC9 (Component ops during migration):** Force-drained resolves all pending
6. **EC10 (Component channel buffers):** Force-drained in each ComponentChannel

---

## Expected Compilation Errors (Before → After)

**Before Phase 2:**
- `shared/src/world/local/local_world_manager.rs:133` - method `remove_entity_channel` not found
- `shared/src/world/local/local_world_manager.rs:134` - value `remote_entity` not found

**After Phase 2:**
- ✅ Compilation errors fixed
- ⚠️ Client-side `todo!()` still present

**After Phase 3:**
- ✅ All `todo!()` macros resolved
- ✅ Feature complete
- ⚠️ May need type adjustments based on actual API

**After Phase 4:**
- ✅ Redirect system fully integrated
- ✅ Retransmissions use correct entity IDs

---

## Notes for Implementation

1. **Don't implement code yet** - this is the WHAT document
2. **Order matters** - follow phases sequentially to maintain compilation
3. **Test incrementally** - run `cargo check` after each phase
4. **Handle HostType** - remember server/client have inverted perspectives
5. **Watch for lifetimes** - EntityChannels have complex internal state
6. **MessageIndex handling** - Use appropriate MessageIndex when setting spawned/inserted state (may use special sentinel value or existing message ID)

---

## Success Criteria

- [ ] `cargo check` passes with no errors
- [ ] Server can migrate RemoteEntity -> HostEntity during delegation
- [ ] Client receives MigrateResponse and migrates HostEntity -> RemoteEntity
- [ ] Component state preserved exactly across migration
- [ ] Buffered operations not lost
- [ ] In-flight messages handled correctly via redirects
- [ ] Entity authority functions correctly post-migration
- [ ] Client can mutate delegated entity
- [ ] Updates replicate to other clients

---

**End of Implementation Plan**

