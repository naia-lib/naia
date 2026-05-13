# Changelog

This page mirrors the repository changelog at the time the book was updated.
For the newest release history, see
[`CHANGELOG.md`](https://github.com/naia-lib/naia/blob/main/CHANGELOG.md).

---

## Unreleased

### Breaking Changes

- `spawn_static_entity` removed. Use `server.spawn_entity(world).as_static()`
  before the first component insert.
- `insert_static_resource` removed. Use
  `server.insert_resource(world, value, true)` for static resources and
  `server.insert_resource(world, value, false)` for dynamic resources.
- Client `WorldEvents<E>` renamed to `Events<E>`.
- `make_room` renamed to `create_room`.
- `resource_count()` renamed to `resources_count()`.
- `room_count()` on `UserRef` renamed to `rooms_count()`.
- Client-side `ReplicationConfig` was replaced by `Publicity`.
- `server.send_message` now returns `Result<(), NaiaServerError>`.
- Server `EntityMut::insert_components` batch variant removed.

### Added

- `entity_is_delegated` predicate on `Server<E>`.
- `EntityMut::as_static()` builder method.
- Server-initiated authority APIs: `give_authority` and `take_authority`.
- Reconnect handling that re-delivers all in-scope entities and replicated
  resources after reconnect.

### Changed

- Internal test/tool crate names were kebab-cased.
- Local-transport hub debug output now uses `log::debug!`.

### Fixed

- Removed UB-prone lifetime transmute in local transport receivers.
- Bounded pending handshake maps to mitigate spoofed source-address floods.
- Fixed orphaned pending-handshake entries on early disconnect.
- Removed dead migration-response stub.
- Added stale-key-safe `user_opt` / `user_mut_opt` accessors.
- Added pending-auth timeout auto-rejection.
- Changed unknown-entity receive handling from panic to warn-and-discard.
- Improved URL parsing panic context.
- Added safety comments to all remaining `unsafe` blocks.
