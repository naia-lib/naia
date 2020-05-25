Some notes:
To run on the web, first, install cargo-web
1. `rustup target add wasm32-unknown-unknown`
2. `cargo install cargo-web`

Build your project for distribution (located at target/deploy)
    `cargo web deploy`

To test the application, run:
    `cargo web start`

    and then open the provided URL in the browser of your choice