# naia

Naia is a cross-platform (including Wasm) client/server networking library for games written in Rust.

It is built on top of https://github.com/connorcarpenter/naia-socket, which provides cross-platform unreliable & unordered messaging (many thanks given to https://github.com/kyren/webrtc-unreliable).

The API is heavily inspired by https://github.com/timetocode/nengi & https://github.com/colyseus/colyseus.

The internals are heavily inspired by the Tribes 2 Networking model: https://www.gamedevs.org/uploads/tribes-networking-model.pdf

Any help is very welcome, please get in touch! I'm still a bit of a Rust noob and this project is pretty intense so I'm open to suggestions/critiques.

## Examples

### Server:

To run a UDP server on Linux: (that will be able to communicate with Linux clients)

    1. cd examples/server
    2. cargo run --features "use-udp"

To run a WebRTC server on Linux: (that will be able to communicate with Web clients)

    1. cd examples/server
    2. cargo run --features "use-webrtc"

### Client:

To run a UDP client on Linux: (that will be able to communicate with a UDP server)

    1. cd examples/client
    2. cargo run

To run a WebRTC client on Web: (that will be able to communicate with a WebRTC server)

    1. Enter in your IP Address at the appropriate spot in examples/client/src/app.rs
    2. cd examples/client
    3. npm install              // should only need to do this once to install dependencies
    4. npm run start            // this will open a web browser, and hot reload


To simply build these examples instead of running them, substitute the above commands like so:

    `cargo build` for `cargo run`, and

    `npm run build` for `npm run start`