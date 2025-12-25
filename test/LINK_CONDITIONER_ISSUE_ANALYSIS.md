# Link Conditioner Test Failures - Deep Investigation

## Problem Summary

5 tests are failing in `time_ticks_transport.rs`, all related to link conditioner functionality:
1. `extreme_jitter_and_reordering_preserve_channel_contracts`
2. `robustness_under_simulated_packet_loss`
3. `out_of_order_packet_handling_does_not_regress_to_older_state`
4. `packet_duplication_does_not_surface_duplicate_events`
5. `deterministic_replay_of_a_scenario`

All tests timeout waiting for messages/updates that should arrive after link conditioner delays.

## Root Cause Analysis

### Packet Flow with Link Conditioner

1. **Server sends message** → `server.send_message()` queues message in Naia's message manager
2. **During tick** → `server.send_all_packets()` is called
3. **Connection sends packet** → `connection.send_packets()` → `connection.send_packet()` → `io.send_packet()` → `hub.send_data()`
4. **Link conditioner queues packet** → `hub.send_data()` sees link conditioner config, calls `link_condition_logic::process_packet()` which queues packet in `server_to_client_queue` with future timestamp (now + latency + jitter)
5. **Delivery attempt** → `deliver_all_queued_packets_to_clients()` is called from `send_data()`, but packet was just queued with future timestamp, so it's not ready yet
6. **Subsequent ticks** → `send_all_packets()` is called, but if there are no new messages to send, `send_packet()` returns `false` immediately, so `send_data()` is never called
7. **Problem**: Since `send_data()` is never called, `deliver_all_queued_packets_to_clients()` is never called, so queued packets never get delivered!

### The Critical Issue

**`deliver_all_queued_packets_to_clients()` is only called from:**
- `hub.send_data()` - when server sends a new packet
- `hub.try_recv_data()` - when server receives a packet

**But if there's nothing new to send/receive, these methods aren't called, so queued packets never get delivered!**

### Code Evidence

In `shared/src/transport/local/hub.rs`:

```rust
pub fn send_data(&self, client_addr: &SocketAddr, bytes: Vec<u8>) -> Result<(), ()> {
    // ...
    let now = Instant::now();
    let mut connections = self.connections.lock().unwrap();
    
    // First, deliver any ready packets from server-to-client queues for all clients
    self.deliver_all_queued_packets_to_clients(&mut connections, &now);
    
    // Then queue the new packet if link conditioner is configured
    if let Some(ref config) = conn.server_to_client_conditioner {
        link_condition_logic::process_packet(config, &mut queue_guard, bytes);
        // Packet is now in queue, will be delivered later
        Ok(())
    }
    // ...
}
```

The problem: `deliver_all_queued_packets_to_clients()` is only called when `send_data()` is called, but `send_data()` is only called when there's a new packet to send.

In `server/src/connection/connection.rs`:

```rust
fn send_packet(...) -> bool {
    if !host_world_events.is_empty()
        || !update_events.is_empty()
        || self.base.message_manager.has_outgoing_messages()
    {
        // Send packet...
        return true;
    }
    false  // No packet to send, so send_data() is never called
}
```

## Solution Options

### Option 1: Call `process_time_queues()` during each tick (RECOMMENDED)

Add a call to `hub.process_time_queues()` in `Scenario::tick()`. This ensures queued packets are delivered every tick, regardless of whether there's new traffic.

**Pros:**
- Simple and direct
- Ensures timely delivery of queued packets
- Doesn't require changes to server/client code

**Cons:**
- Requires access to hub in Scenario (already available)

### Option 2: Call `deliver_all_queued_packets_to_clients()` from `send_all_packets()` even when empty

Modify `connection.send_packets()` to always call a method that processes time queues, even when there's nothing to send.

**Pros:**
- Keeps delivery logic in the transport layer

**Cons:**
- Requires changes to server/client connection code
- Less clean separation of concerns

### Option 3: Periodic background task

Create a background task that periodically processes time queues.

**Pros:**
- Independent of send/receive operations

**Cons:**
- Overkill for test harness
- Adds complexity

## Recommended Fix

**Option 1** is the cleanest solution. The `process_time_queues()` method already exists in `LocalTransportHub` and is designed for this purpose. We just need to call it during each tick.

### Implementation

In `test/src/harness/scenario.rs`, in the `tick()` method:

```rust
fn tick(&mut self) {
    TestClock::advance(TICK_DURATION_MS);
    let now = Instant::now();

    // Process time queues to deliver any ready delayed packets
    // This is critical for link conditioner tests where packets are queued
    // with future timestamps and need to be delivered even when there's
    // no new traffic
    self.hub.process_time_queues();

    // Update all clients and server network
    // ...
}
```

This ensures that every tick, any packets that are ready (their timestamp has passed) will be delivered to the client channels, regardless of whether there's new traffic.

### Current Status

**FIX IMPLEMENTED**: The `process_time_queues()` call has been added to `Scenario::tick()`, but tests are still failing. This suggests there may be additional issues beyond just queue delivery timing.

## Additional Investigation Needed

The fix has been applied, but tests are still timing out. This suggests there may be other issues:

1. **Message Sending**: Are messages actually being sent when link conditioner is configured? Need to verify that `send_all_packets()` is finding and sending messages even when link conditioner queues them.

2. **Time Synchronization**: Is the simulated clock advancing correctly? Are queued packet timestamps being compared correctly against the current time?

3. **Packet Processing**: Are delivered packets being processed correctly by the client? Is `receive_all_packets()` reading from the channel correctly?

4. **Event Collection**: Are messages making it into the client's world events? Is `take_world_events()` collecting them correctly?

## Next Steps

1. Add debug logging to trace packet flow through the system
2. Verify that messages are actually being sent (check if `send_packet()` returns true)
3. Verify that packets are being queued correctly (check queue contents)
4. Verify that packets are being delivered when ready (check if `has_item()` returns true)
5. Verify that delivered packets are being received by the client (check if `receive()` returns packets)
6. Verify that received packets are being processed into messages (check if messages appear in world events)

## Test Evidence

The test `extreme_jitter_and_reordering_preserve_channel_contracts` demonstrates the issue:

1. Messages are sent in `mutate()` block
2. Messages get queued by link conditioner with 10ms latency + 5ms jitter
3. Test waits up to 50 ticks (800ms) for messages to arrive
4. Messages never arrive because `deliver_all_queued_packets_to_clients()` is never called after the initial send

The test even includes a verification that messages work without link conditioner (which passes), confirming the issue is specifically with the link conditioner queue delivery mechanism.
