# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Breaking changes

#### Entities

- **`spawn_static_entity` removed.** Use `server.spawn_entity(world).as_static()` instead.
  Chain `.as_static()` before the first `.insert_component()` call; calling it after will panic.

#### Resources

- **`insert_static_resource` removed.** The `is_static: bool` parameter is now the third
  argument to `insert_resource`:
  - Dynamic: `server.insert_resource(world, value, false)`
  - Static: `server.insert_resource(world, value, true)`

- **`insert_resource` signature changed.** Previous signature was
  `insert_resource(world, value)` (always dynamic). New signature is
  `insert_resource(world, value, is_static: bool)`.

#### Events

- **`WorldEvents<E>` (client) renamed to `Events<E>`.** Update any type annotations or
  `use` imports that referenced `naia_client::WorldEvents`.

#### Rooms

- **`make_room` renamed to `create_room`.** All call sites must be updated.

#### Count methods

- **`resource_count()` renamed to `resources_count()`.** Noun is now plural, consistent
  with `users_count()` and `rooms_count()`.

- **`room_count()` on `UserRef` renamed to `rooms_count()`.** Same pluralisation rule.

#### Client replication config

- **`ReplicationConfig` enum removed from `naia_client`.** Client code that previously
  imported `naia_client::ReplicationConfig` must now import `naia_client::Publicity`
  (re-exported from `naia_shared`). The variants are unchanged: `Private`, `Public`,
  `Delegated`.

#### Messaging

- **`server.send_message` now returns `Result<(), NaiaServerError>`.** Callers that
  previously ignored the return value silently now receive an error if the user is not
  found.

#### EntityMut

- **`EntityMut::insert_components` (batch variant) removed from the server.** Use
  `insert_component` in a loop instead.

### Added

- **`entity_is_delegated` predicate on `Server<E>`.** Convenience equivalent to
  `server.entity_replication_config(e).map_or(false, |c| c.publicity.is_delegated())`.

- **`EntityMut::as_static()` builder method.** Replaces `spawn_static_entity`. Must be
  called before `insert_component` on entities that should be treated as static.

- **`server.give_authority(user_key, entity)` and `entity_mut.give_authority(user_key)`.**
  Server-initiated authority grant. Overrides any current holder (including the same user,
  making it idempotent). Requires the entity to be `Delegated` and in-scope for the target
  user; otherwise a silent no-op. Paired with `take_authority` to reclaim authority.
  Bevy adapter: `entity_commands.give_authority(&mut server, &user_key)`.

- **`server.take_authority(entity)` and `entity_mut.take_authority()`.**
  Reclaims server authority from whatever client currently holds it. Sends
  `SetAuthority(Denied)` to the previous holder and `SetAuthority(Available)` to
  any observers. Bevy adapter: `entity_commands.take_authority(&mut server)`.

- **Reconnect edge-case handling.** Clients that disconnect and reconnect mid-session
  now correctly re-receive all in-scope entities and replicated resources on reconnect.
  Previously, a rapid disconnect/reconnect could leave the client with a stale
  entity set.

### Changed

- **Crate names kebab-cased.** The three internal test/tool crates were renamed for
  consistency with Rust conventions:
  - `naia_npa` → `naia-npa`
  - `naia_bevy_npa` → `naia-bevy-npa`
  - `naia_spec_tool` → `naia-spec-tool`
  Binary file names (snake_case) are unchanged.

- **`transport::local` hub debug output silenced.** Three `println!` calls in
  `LocalTransportHub` were replaced with `log::debug!`. Local-transport noise no
  longer appears in server stdout during tests or production use.

### Fixed (V2 audit, 2026-05-09)

- **CRITICAL — UB transmute in local transport receivers.** `LocalServerReceiver`
  and `LocalClientReceiver` extended the lifetime of a `MutexGuard`-owned buffer
  via `std::mem::transmute`. Both structs now own their last-received payload as
  `Option<Box<[u8]>>`, eliminating the transmute entirely.

- **Handshake address-to-timestamp map unbounded.** Changed from `HashMap` to
  `CacheMap<_, _, MAX_PENDING_CONNECTIONS=1024>` to prevent OOM from spoofed
  source-address floods before authentication completes.

- **Handshake `delete_user` scan-by-value gap.** When a user disconnected before
  completing the identify step, their `been_handshaked_users` entry was left
  orphaned. Fixed with a `retain()` scan on `None` address.

- **`on_delivered_migrate_response` dead stub removed.** The function had two
  incorrect magic values in its TODO body; it was not called anywhere. Removed to
  avoid a future confusion hazard.

- **`user()` panics on stale key.** Added `user_opt` and `user_mut_opt` on
  `Server<E>`, `WorldServer<E>`, and `MainServer` so callers can avoid the panic
  when a `UserKey` may be stale.

- **Pending-auth timeout.** Connections that completed the network handshake but
  whose application never called `accept_connection` / `reject_connection` within
  `ServerConfig::pending_auth_timeout` (default 10 s) are now auto-rejected with
  a warning log.

- **`host_engine` receive on unknown entity panicked.** Changed to `warn!` + discard;
  reordered packets from a lagging client after entity despawn no longer crash the server.

- **`url_str_to_addr` panics lacked context.** All five `panic!("")` calls now
  include the offending URL string.

- **Safety comments on all `unsafe` blocks.** 20 unsafe sites across server, client,
  shared, socket, and adapter crates now carry `// Safety:` comments explaining
  the invariant that justifies each unsafe use.
