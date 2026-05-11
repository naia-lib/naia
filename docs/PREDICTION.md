# Client-Side Prediction and Rollback in naia

This guide explains how to implement client-side prediction with rollback
correction using naia's `TickBuffered` channels, `CommandHistory`, and
`local_duplicate()`. The techniques shown are drawn from the working Bevy demo
at `demos/bevy/`.

---

## Mental model

The server is authoritative. The client runs **ahead** of the server by
approximately half the round-trip time (RTT/2) so that commands the client
sends today arrive at the server in time for the server tick they belong to.

```
                 client                     server
                   │                           │
  tick 47 ──────── │──── KeyCommand(t=47) ────▶│
  tick 48 ──────── │                           │
  tick 49 ──────── │◀─── Position(t=47) ───────│  (arrives ~RTT later)
                   │                           │
```

The client doesn't wait for the server's reply before moving the player —
it applies the input **immediately** to a local *predicted* copy of the entity.
When the authoritative correction arrives, the client:

1. Snaps the predicted state to the authoritative server state.
2. Re-simulates ("replays") every command issued after that correction tick.

This hides latency and keeps the player's avatar feeling responsive at any RTT.

---

## The five building blocks

### 1. `TickBuffered` channel — stamped input delivery

Declare a `TickBuffered` channel in your shared protocol:

```rust
// shared/src/channels.rs
protocol.add_channel::<PlayerCommandChannel>(ChannelSettings {
    mode: ChannelMode::TickBuffered(TickBufferSettings::default()),
    direction: ChannelDirection::ClientToServer,
});
```

`TickBuffered` attaches the **client tick number** to every message. The server
reads those messages with `receive_tick_buffer_messages(&server_tick)` — it
only sees commands whose stamp matches the current server tick, so input arrives
at exactly the right simulation step even under jitter.

### 2. `CommandHistory` — the replay buffer

```rust
use naia_client::CommandHistory;

// In your client resources:
pub command_history: CommandHistory<KeyCommand>,
```

`CommandHistory::new(128)` keeps the last 128 ticks of input. Choose a depth
of at least `ceil(max_expected_RTT_ticks × 2)`. Too shallow and corrections
outside the window cause a visible snap; too deep wastes memory.

```rust
// At startup / in Global::default():
command_history: CommandHistory::new(128),
```

### 3. `local_duplicate()` — creating the predicted entity

When the server assigns an entity to the local player, clone it into a local
*predicted* counterpart:

```rust
// In message_events, when EntityAssignment.assign == true:
let prediction_entity = commands.entity(confirmed_entity).local_duplicate();
// prediction_entity has all Replicate components copied but is client-local.
global.owned_entity = Some(OwnedEntity { confirmed: confirmed_entity,
                                          predicted: prediction_entity });
```

`local_duplicate()` copies every `Replicate` component so the prediction starts
in sync with the server's last known state.

### 4. Per-tick loop — record → send → apply

In your `ClientTickEvent` handler, for each tick:

```rust
pub fn tick_events(
    mut client: Client<Main>,
    mut global: ResMut<Global>,
    mut tick_reader: ResMut<Messages<ClientTickEvent<Main>>>,
    mut position_query: Query<&mut Position>,
) {
    let Some(predicted_entity) = global.owned_entity.as_ref()
        .map(|e| e.predicted) else { return; };
    let Some(command) = global.queued_command.take() else { return; };

    for event in tick_reader.drain() {
        let client_tick = event.tick;

        // 1. Guard: don't overflow the history window.
        if !global.command_history.can_insert(&client_tick) { continue; }

        // 2. Record.
        global.command_history.insert(client_tick, command.clone());

        // 3. Send (with tick stamp — arrives at server at the right tick).
        client.send_tick_buffer_message::<PlayerCommandChannel, KeyCommand>(
            &client_tick, &command,
        );

        // 4. Apply locally (prediction — no server round-trip yet).
        if let Ok(mut position) = position_query.get_mut(predicted_entity) {
            shared_behavior::process_command(&command, &mut position);
        }
    }
}
```

### 5. Correction handler — rollback + re-simulate

When the server sends an authoritative `UpdateComponent` for the confirmed
entity, roll the predicted entity back and replay:

```rust
pub fn update_component_events(
    mut global: ResMut<Global>,
    mut position_event_reader: ResMut<Messages<UpdateComponentEvent<Main, Position>>>,
    mut position_query: Query<&mut Position>,
) {
    let Some(owned) = &global.owned_entity else { return; };

    // Find the latest server correction tick for the owned entity.
    let mut latest_tick: Option<Tick> = None;
    for event in position_event_reader.drain() {
        if event.entity == owned.confirmed {
            match latest_tick {
                Some(t) if sequence_greater_than(event.tick, t) => {}
                _ => latest_tick = Some(event.tick),
            }
        }
    }

    let Some(server_tick) = latest_tick else { return; };

    if let Ok([server_pos, mut client_pos]) =
        position_query.get_many_mut([owned.confirmed, owned.predicted])
    {
        // Step A: snap prediction to authoritative state.
        client_pos.mirror(&*server_pos);

        // Step B: re-simulate every command since that server tick.
        for (_tick, command) in global.command_history.replays(&server_tick) {
            shared_behavior::process_command(&command, &mut client_pos);
        }
    }
}
```

`command_history.replays(&server_tick)` returns all commands stored *after*
`server_tick` in sequence order, so the re-simulation starts from the
authoritative snapshot and runs forward to the present client tick.

---

## Server side — reading stamped input

```rust
// In tick_events on the server:
let mut messages = server.receive_tick_buffer_messages(&server_tick);
for (_user_key, command) in messages.read::<PlayerCommandChannel, KeyCommand>() {
    let Some(entity) = command.entity.get(&server) else { continue; };
    let Ok(mut position) = position_query.get_mut(entity) else { continue; };
    shared_behavior::process_command(&command, &mut position);
}
```

`receive_tick_buffer_messages` only yields commands whose client-tick stamp
matches the current `server_tick`. Commands that arrived early are held in the
tick buffer; late commands are discarded.

---

## Tuning the prediction window

The depth of `CommandHistory` and the `TickBufferSettings` interact:

- **`CommandHistory::new(N)`** — keep at most N ticks of history. Set N ≥ 2 ×
  max RTT in ticks. At 20 Hz and 200 ms max RTT that's ≥ 8 ticks; 128 is a
  safe default.
- **`TickBufferSettings::default()`** — the tick buffer accepts commands within
  a small window around the current server tick. For high-jitter links, widen
  the window.
- **Tick rate** — lower tick rates (e.g. 20 Hz) increase the granularity of
  prediction mismatches. Higher rates reduce visible snap but increase CPU and
  bandwidth.

---

## Full working example

See `demos/bevy/client/src/systems/events.rs` for the complete prediction loop
and `demos/bevy/server/src/systems/events.rs` for the server's tick-buffer
read path. The shared movement logic lives in
`demos/bevy/shared/src/behavior.rs`.
