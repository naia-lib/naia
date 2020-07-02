[![Build Status](https://img.shields.io/circleci/project/github/naia-rs/naia.svg)](https://circleci.com/gh/naia-rs/naia)
[![Latest Version](https://img.shields.io/crates/v/naia-server.svg)](https://crates.io/crates/naia-server)
[![API Documentation](https://docs.rs/naia-server/badge.svg)](https://docs.rs/naia-server)
![](https://tokei.rs/b1/github/naia-rs/naia)
[![MIT/Apache][s3]][l3]

[s3]: https://img.shields.io/badge/license-MIT%2FApache-blue.svg
[l3]: docs/LICENSE-MIT

# naia
a *n*etworking *a*rchitecture for *i*nteractive *a*pplications

A cross-platform (currently WebAssembly & Linux) networking engine that intends to make multiplayer game development in Rust dead simple and lightning fast.

At the highest level, you register Event and Entity implementations in a module shared by Client & Server. Then, naia will facilitate sending/receiving those Events between Client & Server, and also keep a pool of tracked Entities synced with each Client for whom they are "in-scope". Entities are "scoped" to Clients with whom they share the same Room, as well as being sufficiently customizable to, for example, only keep Entities persisted & synced while within a Client's viewport or according to some other criteria.

It is built on top of [naia-socket](https://github.com/naia-rs/naia-socket), which provides cross-platform unreliable & unordered messaging.

The API is heavily inspired by the [Nengi.js](https://github.com/timetocode/nengi) & [Colyseus](https://github.com/colyseus/colyseus) Javascript multiplayer networking libraries. The internals follow the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf) fairly closely.

Thank very much to Kyren for support & [webrtc-unreliable](https://github.com/kyren/webrtc-unreliable), and to the [Laminar](https://github.com/amethyst/laminar) authors, for the cannibalized code within.

Any help is very welcome, please get in touch! I'm still very new to Rust and this project is overly ambitious, and so I intend to be very open to any criticism / feedback in order to better this project.

## Examples

More comprehensive documentation / tutorials are on their way, but for now, the best way to get started with naia is to go through the single example, which demonstrates most of the functionality.

### Server:

To run the UDP server example on Linux: (that will be able to communicate with Linux clients)

    1. cd examples/server
    2. cargo run --features "use-udp"

To run the WebRTC server example on Linux: (that will be able to communicate with Web clients)

    1. cd examples/server
    2. cargo run --features "use-webrtc"

### Client:

To run the UDP client example on Linux: (that will be able to communicate with a UDP server)

    1. cd examples/client
    2. cargo run

To run the WebRTC client example on Web: (that will be able to communicate with a WebRTC server)

    1. cargo install cargo-web  // should only need to do this once if you haven't already
    2. Enter in your IP Address at the appropriate spot in examples/client/src/app.rs
    3. cd examples/client
    4. npm install              // should only need to do this once to install dependencies
    5. npm run start            // this will open a web browser, and hot reload
    6. Open your debug console to see communications from the server
