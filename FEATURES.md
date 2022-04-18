# Features

## Current

* [x] UDP / WebRTC Server implementation
* [x] Linux / Wasm Client implementation
* [x] Heartbeats
* [x] Host timeout detection
* [x] Basic DoS mitigation
* [x] Connection / Disconnection events
* [x] Customizable Client authentication
* [x] Unguaranteed & guaranteed Messages sent between hosts
* [x] Entities & their Components sync with Clients when "in scope"
* [x] Rooms restrict syncing to their contained Users & Entities
* [x] Customizable scoping function for advanced usage
* [x] RTT estimations
* [x] Client Tick events
* [x] Synced Tick between Server/Client

## Planned
This list is not sorted by order of priority

* [ ] Integration & Unit Tests
* [ ] Better error handling
* [ ] Load Testing & Benchmarks
* [ ] Congestion Control
* [ ] Custom Property read/write implementation
* [ ] "Deep" Replica property syncing
* [ ] Ordered Guaranteed Messages?
* [ ] Update Priority (indicates certain updates should be sent earlier than others)
* [ ] Dynamic Update Priority based on scope evaluation (conditionally raise priority)
* [ ] Set independent Entity/Component update rate
* [ ] Horizontally scale Servers
* [ ] Support Debugging / Logging / Metrics visualizations
* [ ] Bitwise (as opposed to current "Bytewise") reading/writing of messages, to save bandwidth
* [ ] File-like API for streaming assets / caching on client

## Planned for [naia-socket]

These planned changes for naia-socket will bring new features to naia as well.

* [ ] Integration & Unit Tests
* [ ] Better error handling
* [ ] Load Testing & Benchmarks
* [ ] Android-compatible Client Socket
* [ ] iOS-compatible Client Socket
