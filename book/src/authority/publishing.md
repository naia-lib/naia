# Entity Publishing

Entity publishing controls whether a server-spawned entity is replicated to
clients via the `ReplicationConfig` API.

---

## ReplicationConfig variants

| Variant | Effect |
|---------|--------|
| `ReplicationConfig::default()` | Entity is replicated to in-scope users (default) |
| `ReplicationConfig::delegated()` | Entity is marked as eligible for client authority requests |

Entities are replicated by default as soon as they are in a user's scope (shared
room + UserScope include). No explicit publish step is needed.

---

## Controlling replication per entity

To temporarily stop replicating an entity without removing it from scope, set
its priority gain to `0.0`:

```rust
server.global_entity_priority_mut(entity).set_gain(0.0);
```

To remove it from scope entirely, exclude it from the user's scope and / or
remove it from all shared rooms.

---

## Replicated resources

Replicated resources are always visible to all connected users — they bypass
room and scope logic. See [Entity Replication](../concepts/replication.md#replicated-resources).
