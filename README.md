# gaia-client

The plan is to provide a cross-platform (including Wasm) client/server networking library for games written in Rust.

It will be built on top of https://github.com/connorcarpenter/gaia-socket, which will provide unreliable & unordered messaging.

The API will be heavily inspired by https://github.com/timetocode/nengi & https://github.com/colyseus/colyseus.

The internals will be heavily inspired by the Tribes 2 Networking model: https://www.gamedevs.org/uploads/tribes-networking-model.pdf and/or the Quake 3 model: https://fabiensanglard.net/quake3/network.php

Any help is very welcome, please get in touch! I'm still quite a Rust noob and this project is pretty intense so I'm open to suggestions/critiques.