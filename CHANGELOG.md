# Changelog

## 2026-01-06 - Bevy adapter fixes and build/task improvements

- adapters/bevy/shared: migrate from Bevy events API to messages
  - `HostSyncEvent` now derives `Message` instead of `Event`.
  - Replaced `EventWriter<HostSyncEvent>` with `MessageWriter<HostSyncEvent>`.
  - Replaced deprecated `.add_event::<HostSyncEvent>()` with `.add_message::<HostSyncEvent>()`.
  - Converted free functions to systems using `IntoSystem::into_system(...)` and applied `.in_set(...)`.
  - Removed incorrect `.chain()` usage when registering systems.
  - Fixed `component_id` dereference in `world_proxy.rs` to call `get_info(*component_id)`.
