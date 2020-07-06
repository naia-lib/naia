# Features

## Current

* [x] UDP / WebRTC Server implementation
* [x] Linux / Wasm Client implementation
* [x] Heartbeats
* [x] Host timeout detection
* [x] Basic DoS mitigation
* [x] Connection / Disconnection events
* [x] Customizable Client authentication
* [x] Unguaranteed & guaranteed Events sent between hosts
* [x] Entities sync with Clients when "in scope"
* [x] Rooms restrict syncing to their contained Users & Entities
* [x] Customizable scoping function for advanced usage
* [x] Rtt estimations

## Planned
This list is not sorted by order of priority

* [ ] Integration & Unit Tests
* [ ] Better error handling
* [ ] Load Testing & Benchmarks
* [ ] Congestion Control
* [ ] Client Tick events
* [ ] Synced Tick between Server/Client
* [ ] Custom Property read/write implementation
* [ ] "Deep" Entity property syncing
* [ ] Support Client prediction of Entities
* [ ] Support Client-side iterpolation of Entity properties
* [ ] Ordered Guaranteed Events?
* [ ] Send consecutive copies of Events (see Tribes 2 Networking Model's "MoveManager")
* [ ] Event/Entity Priority (indicates certain updates should be sent earlier than others)
* [ ] Dynamic Event/Entity Priority based on scope evaluation (conditionally raise priority on Entities)
* [ ] Set independent Entity update rate
* [ ] Horizontally scale Servers
* [ ] Support Debugging / Logging / Metrics visualizations
* [ ] Bitwise (as opposed to current "Bytewise") reading/writing of messages, to save bandwidth
* [ ] File-like API for streaming assets / caching on client

## Planned for [naia-socket](https://github.com/naia-rs/naia-socket)

These planned changes for naia-socket will bring new features to naia as well.

* [ ] Integration & Unit Tests
* [ ] Better error handling
* [ ] Load Testing & Benchmarks
* [ ] Optionally use stdweb instead of web_sys for Web build
* [ ] Server socket can run on a separate thread
* [ ] Udp Server & Linux Client uses DTLS to reach parity with WebRTC
* [ ] Windows-compatible Client Socket
* [ ] MacOS-compatible Client Socket
* [ ] Android-compatible Client Socket
* [ ] iOS-compatible Client Socket
