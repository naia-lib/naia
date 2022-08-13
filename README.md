[![Latest Version](https://img.shields.io/crates/v/naia-server.svg)](https://crates.io/crates/naia-server)
[![API Documentation](https://docs.rs/naia-server/badge.svg)](https://docs.rs/naia-server)
![](https://tokei.rs/b1/github/naia-rs/naia)
[![Discord chat](https://img.shields.io/discord/764975354913619988.svg?label=discord%20chat)](https://discord.gg/fD6QCtX)
[![MIT/Apache][s3]][l3]

[s3]: https://img.shields.io/badge/license-MIT%2FApache-blue.svg
[l3]: docs/LICENSE-MIT

# naia
a *n*etworking *a*rchitecture for *i*nteractive *a*pplications

A cross-platform (including Wasm!) networking engine that intends to make multiplayer game development in Rust dead simple and lightning fast.

naia helps you to easily define a common, shared Protocol that allows Server & Client to exchange information. Then, naia facilitates sending/receiving parts of that Protocol as reliable/unreliable Messages between Server & Client, and also keeps a pool of tracked Entities synced with each Client for whom they are "in-scope". Entities are "scoped" to Clients with whom they share the same Room, as well as being sufficiently customizable to, for example, only keep Entities persisted & synced while within a Client's viewport or according to some other criteria.

The API is heavily inspired by the [Nengi.js](https://github.com/timetocode/nengi) & [Colyseus](https://github.com/colyseus/colyseus) Javascript multiplayer networking libraries. The internals follow the [Tribes 2 Networking model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf) fairly closely.

Thank very much to Kyren for support & [webrtc-unreliable](https://github.com/kyren/webrtc-unreliable), and to the [Laminar](https://github.com/amethyst/laminar) authors, for the cannibalized code within.

Any help is very welcome, please get in touch! I am open to any criticism / feedback in order to better this project.

Currently guaranteed to work on Web & Linux, although Windows & MacOS have been reported working as well. Please file issues if you find inconsistencies and I'll do what I can.

## Demos

More comprehensive documentation / tutorials are on their way, but for now, the best way to get started with naia is to go through the basic demo, which demonstrates most of the functionality.

### Server:

To run the UDP server demo on Linux: (that will be able to communicate with Linux clients)

    1. cd /naia/demos/basic/server
    2. cargo run --features "use-udp"

To run the WebRTC server demo on Linux: (that will be able to communicate with Web clients)

    1. // go to (https://docs.rs/openssl/latest/openssl/) to install openssl on your machine
    2. cd /naia/demos/basic/server
    3. cargo run --features "use-webrtc"

### Client:

To run the UDP client demo on Linux: (that will be able to communicate with a UDP server)

    1. cd /naia/demos/basic/client/wasm_bindgen
    2. cargo run

To run the WebRTC client demo on Web: (that will be able to communicate with a WebRTC server)

    1. cargo install cargo-web  // should only need to do this once if you haven't already
    2. cargo install cargo-make // should only need to do this once if you haven't already
    3. cd /naia/demos/basic/client/wasm_bindgen
    4. make serve
    5. Web page will be blank - check debug console to see communications from the server

### Known Issues

To run a miniquad client you will require the following be installed

    sudo apt-get install libxi-dev libgl1-mesa-dev