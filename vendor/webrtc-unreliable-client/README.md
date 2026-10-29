# webrtc-unreliable-client
![](https://tokei.rs/b1/github/naia-lib/naia-unreliable-client)
[![MIT/Apache][s3]][l3]

[s3]: https://img.shields.io/badge/license-MIT%2FApache-blue.svg
[l3]: docs/LICENSE-MIT

Just enough hacks to connect a native client to a [webrtc-unreliable](https://github.com/triplehex/webrtc-unreliable) server.

At first this was going to just wrap [webrtc-rs](https://github.com/webrtc-rs/webrtc), but I experienced absurd compile times going that route .. Knowing that I very likely needed a much smaller subset of functionality to get an unreliable, unordered datachannel set up to connect to a `webrtc-unreliable` server, I cloned all `webrtc-rs` and started to cut it down. I'm not so sure this was the correct path. Protocols like DTLS, SCTP, SDP really don't have a lot of fat to trim, so it seems. However, I believe this is progress.

If anyone knows of a good way to anayze during run-time which functions are never called, please get in touch!

## Runtime

This crate is executor-agnostic: it spawns tasks with `smol::spawn` on the
global executor and performs I/O with `async-net`, so any executor driving the
returned futures (for example `smol::block_on`) can run it. It does not start
or require a `tokio` runtime.

