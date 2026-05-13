# Summary

[Introduction](introduction.md)

# Getting Started

- [Why naia?](getting-started/why-naia.md)
- [Installation](getting-started/installation.md)
- [Bevy Quick Start](getting-started/bevy-quickstart.md)
- [Your First Server](getting-started/first-server.md)
- [Your First Client](getting-started/first-client.md)
- [Running the Demos](getting-started/demos.md)

# Core Concepts

- [The Shared Protocol](concepts/protocol.md)
- [Entity Replication](concepts/replication.md)
- [Messages & Channels](concepts/messages.md)
- [Rooms & Scoping](concepts/rooms.md)
- [Tick Synchronization](concepts/ticks.md)
- [Connection Lifecycle](concepts/connection.md)

# Authority & Ownership

- [Server Authority Model](authority/server-authority.md)
- [Client-Owned Entities](authority/client-owned.md)
- [Authority Delegation](authority/delegation.md)
- [Entity Publishing](authority/publishing.md)

# Advanced Features

- [Client-Side Prediction & Rollback](advanced/prediction.md)
- [Lag Compensation with Historian](advanced/historian.md)
- [Priority-Weighted Bandwidth](advanced/bandwidth.md)
- [Delta Compression](advanced/delta-compression.md)
- [zstd Compression & Dictionary Training](advanced/compression.md)
- [Request / Response](advanced/request-response.md)

# Transports

- [Overview](transports/overview.md)
- [Native UDP](transports/udp.md)
- [WebRTC (Native + Browser)](transports/webrtc.md)
- [Local (In-Process)](transports/local.md)
- [Writing a Custom Transport](transports/custom.md)

# Without Bevy

- [Core API Overview](adapters/overview.md)
- [Macroquad](adapters/macroquad.md)
- [Writing Your Own Adapter](adapters/custom.md)

# Performance & Diagnostics

- [Bandwidth Budget Analysis](perf/bandwidth.md)
- [Connection Diagnostics](perf/diagnostics.md)
- [Benchmarking](perf/benchmarks.md)
- [Scaling Considerations](perf/scaling.md)

# Reference

- [Feature Matrix](reference/features.md)
- [Security & Trust Model](reference/security.md)
- [Comparing naia to Alternatives](reference/comparison.md)
- [Bevy Adapter Deep Dive](adapters/bevy.md)
- [Glossary](reference/glossary.md)
- [API Docs (docs.rs)](reference/api.md)
- [FAQ](reference/faq.md)
- [Changelog](reference/changelog.md)
