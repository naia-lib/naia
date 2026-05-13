# Server Authority Model

naia's default replication model is server-authoritative: the server owns the
canonical state for ordinary replicated entities and resources, and clients
receive the subset the server places in their scope.

That default is not the whole authority model. Protocols can opt into
client-authoritative entities, and server-owned entities/resources can be made
delegable. Use server authority as the baseline, then opt into the more flexible
paths where the gameplay actually needs them.

---

## What server authority means

- The server usually spawns, updates, and despawns replicated entities.
- Clients never write to server-owned undelegated entities directly.
- All client inputs travel to the server through typed messages or
  `TickBuffered` channels; the server applies them and the resulting state
  update replicates back to clients.
- If the protocol enables client-authoritative entities, clients may create
  entities that replicate to the server.
- The server can grant a client *temporary* write authority over a specific
  entity or resource via [Authority Delegation](delegation.md), but retains the
  right to revoke it at any time.

---

## Why server authority?

Server authority prevents a class of cheats where a client modifies local game
state (position, health, score) and expects the server to accept it. Without a
validation point, any client can claim any position, and that makes for a very
short speedrun to chaos.

> **Warning:** naia does not validate client mutations even when authority is delegated. The
> server must validate all client-originated state before applying it to
> authoritative game state.

## Reclaiming authority

The server can revoke a client's authority at any time by calling
`entity_take_authority`:

```rust
// Server forcibly reclaims authority over a delegated entity.
server.entity_take_authority(&mut world, &entity);
```

After this call the entity returns to `Available` status. The client that held
authority receives a notification that it was revoked.

> **Tip:** Revoke authority automatically when a player disconnects. An entity
> stuck in `Granted` state after a disconnect is a resource leak — the authority
> slot can never be reclaimed without a server restart.

---

## NAT traversal and P2P

naia is server-authoritative by design — NAT traversal and peer-to-peer
hole-punching are intentionally out of scope.

For P2P networking (e.g. browser-to-browser direct connections for a rollback
fighting game), the recommended Rust/Wasm ecosystem tools are:

- **[matchbox_socket](https://github.com/johanhelsing/matchbox)** — async WebRTC
  signaling for P2P connections.
- **[GGRS / bevy_ggrs](https://github.com/gschup/ggrs)** — GGPO-style rollback
  netcode on top of matchbox.

These are complementary: a game can use naia for server→client replication
(lobby, world state) and GGRS for fast-path P2P match simulation in parallel.
