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
* [x] Replicates sync with Clients when "in scope"
* [x] Rooms restrict syncing to their contained Users & Replicates
* [x] Customizable scoping function for advanced usage
* [x] Rtt estimations
* [x] Client Tick events
* [x] Synced Tick between Server/Client
* [x] Support Client prediction of Replicates
* [x] Send consecutive copies of Events (see Tribes 2 Networking Model's "MoveManager")

## Planned
This list is not sorted by order of priority

* [ ] Integration & Unit Tests
* [ ] Better error handling
* [ ] Load Testing & Benchmarks
* [ ] Congestion Control
* [ ] Custom Property read/write implementation
* [ ] "Deep" Replicate property syncing
* [ ] Ordered Guaranteed Events?
* [ ] Event/Replicate Priority (indicates certain updates should be sent earlier than others)
* [ ] Dynamic Event/Replicate Priority based on scope evaluation (conditionally raise priority on Replicates)
* [ ] Set independent Replicate update rate
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
