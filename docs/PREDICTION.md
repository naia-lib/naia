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

## Multi-entity rollback

The guide above predicts a single entity. Most games predict several
simultaneously — the local player plus any client-owned objects they control.
The key rule: **entities that can physically interact must be predicted
together in the same simulation step**, otherwise collisions and constraints
computed during replay will be incorrect.

### Each predicted entity needs its own confirmed/predicted pair

```rust
pub struct OwnedEntities {
    // Add one pair per independently predicted entity.
    pub player:  OwnedEntity, // { confirmed, predicted }
    pub shield:  OwnedEntity,
}
```

Each pair requires its own `CommandHistory` if the entity takes different
inputs:

```rust
pub struct Global {
    pub player_history: CommandHistory<PlayerCommand>,
    pub shield_history: CommandHistory<ShieldCommand>,
}
```

### Replay all entities together per tick

The correction handler determines the earliest server tick that needs
re-simulation. The replay loop then re-runs **all** entities side-by-side for
every tick in the window:

```rust
// Correction handler — find the earliest tick needing replay
// (there may be corrections for multiple entities in one frame;
//  see "Batching corrections" below).
let rollback_from: Tick = earliest_correction_tick;

// Replay loop — advance every entity together each tick
for (tick, player_cmd) in global.player_history.replays(&rollback_from) {
    // Apply player command to the predicted player entity
    if let Ok(mut pos) = position_query.get_mut(owned.player.predicted) {
        shared_behavior::apply_player_command(&player_cmd, &mut pos);
    }
    // Apply shield command to the predicted shield entity
    if let (Ok(shield_cmd), Ok(mut shield_pos)) = (
        global.shield_history.get(tick),
        position_query.get_mut(owned.shield.predicted),
    ) {
        shared_behavior::apply_shield_command(&shield_cmd, &mut shield_pos);
    }

    // IMPORTANT: tick the physics/collision step for ALL entities here,
    // so that interactions between them are computed correctly for this tick.
    physics_world.step();
}
```

Processing all entities inside the same per-tick loop — rather than replaying
one entity fully before starting the next — is what makes multi-entity
interactions (collisions, pushes, overlaps) come out right.

### Remote avatar proxies during replay

Other players appear on the client as server-replicated entities. During
replay you need a command for them too, or their position in the physics step
will be stale. The safest heuristic: give each proxy its **last received
command** for the first N replay ticks, then freeze it (zero velocity, hold
position) for any remaining ticks.

```rust
// During each replay tick i (0-indexed from rollback_from):
let effective_lead = (total_replay_ticks as f32 * lead_scale).ceil() as u32;
let proxy_cmd = if i < effective_lead {
    // Let the proxy keep moving with its last known intent.
    remote_entities[j].last_command.clone()
} else {
    // Freeze: stop giving it commands so it does not drift further.
    Command::idle()
};
apply_to_proxy(proxy_entity, proxy_cmd, &mut position_query);
```

The `lead_scale` (typically `1.0`) and a hard cap (`max_lead_ticks`) let you
tune the trade-off: a longer lead produces more accurate proxy replay at the
cost of more simulation work; a shorter lead freezes proxies sooner but avoids
phantom-movement artifacts in the render.

---

## Misprediction correction

Every `UpdateComponentEvent` for the local player's confirmed entity is a
correction signal — the server's authoritative state differed from what the
client predicted. There are two strategies for handling the visual result.

### Strategy A — Instant snap (simplest)

Snap the predicted entity directly to the server value, then replay:

```rust
// Step A: snap predicted entity to server state.
predicted_pos.mirror(&*server_pos);

// Step B: replay all commands after the correction tick.
for (_tick, cmd) in global.command_history.replays(&server_tick) {
    shared_behavior::process_command(&cmd, &mut predicted_pos);
}
```

This is correct but can produce a visible pop on high-latency links where
corrections are large. It is the right default for any game where position
errors are small (tightly timed tick rates, low-latency networks).

### Strategy B — Smooth error interpolation (production)

Rather than snapping the *rendered* position instantly, record the
pre-rollback render position, run the rollback, and then blend the visual
position from the old position to the new one over a short window (e.g.
150–250 ms):

```rust
// Before rollback:
let pre_rollback_render_pos = render_position.current();

// Run the rollback (snap + replay — same as Strategy A).
predicted_pos.mirror(&*server_pos);
for (_tick, cmd) in global.command_history.replays(&server_tick) {
    shared_behavior::process_command(&cmd, &mut predicted_pos);
}

// After rollback — compute the error and begin interpolating it away.
let post_rollback_render_pos = interpolate_from_physics(&predicted_pos);
let error = pre_rollback_render_pos - post_rollback_render_pos;
render_position.begin_error_correction(error, CORRECTION_DURATION_MS);
```

Each frame, the renderer applies a decaying fraction of `error` on top of the
physically-correct position:

```rust
// In the render system each frame:
let alpha = elapsed_ms / CORRECTION_DURATION_MS; // 0.0 → 1.0
let visual_pos = physics_pos + error * (1.0 - smooth_step(alpha));
```

This hides the snap entirely for corrections smaller than one character
diameter and is imperceptible for larger ones at the recommended 250 ms
window. The simulation is always physically correct; only the screen position
is blended.

### Threshold-based early exit

On a well-tuned server with a low-jitter network, most corrections are
sub-pixel. Skipping rollback for tiny corrections reduces CPU cost and
eliminates micro-jitter from floating-point rounding:

```rust
const CORRECTION_THRESHOLD_SQ: f32 = 0.01 * 0.01; // 1 cm²

let delta = server_pos.value() - predicted_pos.value();
if delta.length_squared() < CORRECTION_THRESHOLD_SQ {
    return; // close enough — skip the rollback for this frame
}

// Otherwise proceed with snap + replay.
```

Apply the threshold only to the *physics* snap decision; always update the
confirmed entity from the server value regardless. The confirmed entity is the
ground truth the next rollback will start from.

---

## Batching corrections from the same frame

Multiple `UpdateComponentEvent`s can arrive in the same frame — for example,
a position correction and a velocity correction both generated on the same
server tick. Running a separate rollback for each is wasteful and can produce
ordering artifacts.

**The pattern:** accumulate the *earliest* correction tick across all
component events in the frame, then run one rollback at the end.

```rust
// Drain ALL correction events first, tracking only the earliest tick.
let mut rollback_tick: Option<Tick> = None;

for event in position_events.drain() {
    if event.entity == owned.confirmed {
        rollback_tick = Some(match rollback_tick {
            Some(t) if sequence_greater_than(t, event.tick) => event.tick,
            Some(t) => t,
            None => event.tick,
        });
    }
}

for event in velocity_events.drain() {
    if event.entity == owned.confirmed {
        rollback_tick = Some(match rollback_tick {
            Some(t) if sequence_greater_than(t, event.tick) => event.tick,
            Some(t) => t,
            None => event.tick,
        });
    }
}

// Run exactly one rollback, from the earliest correction tick.
if let Some(from_tick) = rollback_tick {
    run_rollback(from_tick, &mut global, &mut position_query, &mut velocity_query);
}
```

**System ordering matters.** Drain all correction events in one system, queue
the earliest tick, and run the replay in a subsequent system — not inline
inside each event handler. If you run a rollback inside the `position` event
handler before the `velocity` handler has run, the second rollback overwrites
the result of the first:

```
HandleWorldEvents  ← drain position + velocity events, store earliest tick
Rollback           ← execute one rollback from earliest tick
```

---

## Tick-buffer miss

`TickBuffered` channels guarantee delivery but not timing. A command sent for
client tick T may arrive at the server after tick T has already executed. When
that happens the server's tick-buffer discards the command silently — there is
no error or event on either side.

**How the client detects a miss:**

1. Client sends command for tick T, applies it locally to the predicted entity.
2. The command arrives late; the server runs tick T without it.
3. The server replicates the resulting (command-less) state on the next
   send cycle.
4. The client receives an `UpdateComponentEvent` for the confirmed entity — the
   server's position differs from the predicted one.
5. The client's normal correction handler fires, rolls back to T, and replays.

From the client's perspective a tick-buffer miss is **indistinguishable from
an ordinary misprediction**. The rollback mechanism handles it correctly
without any special case.

**How to diagnose misses during development:**

A sudden cluster of corrections for the same entity across several consecutive
ticks (rather than isolated one-off corrections) is the signature of a
tick-buffer miss. Add a counter in your `UpdateComponentEvent` handler and log
when you see more than N corrections in M ticks for the same entity. If you
see systematic misses, the usual causes are:

- Client tick leading too little — increase the client tick advance so commands
  arrive earlier. Check `ClientConfig::minimum_latency`.
- Server tick rate too high relative to `TickBufferSettings` acceptance window.
  Widen the window or reduce the tick rate.
- Network jitter spikes — the `LinkConditionerConfig` presets let you reproduce
  this locally; use `poor_condition()` in development to verify your correction
  handler is robust.

**`CommandHistory` depth and the miss window:**

A tick-buffer miss triggers a rollback from the missed tick. Your
`CommandHistory` must be deep enough to hold commands back to that tick:

```
required_depth ≥ ceil(max_RTT / tick_interval) × 2
```

At 25 Hz (40 ms/tick) with 300 ms max RTT: ≥ ceil(300 / 40) × 2 = 16 ticks.
The default of 128 is deliberately generous; reduce it only after profiling
shows the memory cost matters.

---

## Full working example

See `demos/bevy/client/src/systems/events.rs` for the complete prediction loop
and `demos/bevy/server/src/systems/events.rs` for the server's tick-buffer
read path. The shared movement logic lives in
`demos/bevy/shared/src/behavior.rs`.
