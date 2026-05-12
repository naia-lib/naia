# Try It In Your Browser

naia is unique in the Rust game networking ecosystem in having a WebRTC
transport that runs natively in the browser. The demo below runs a live naia
session directly on this page — no installation required.

> **Note:** The live demo uses naia's `transport_local` running entirely in WASM, so no
> backend server is needed. Both the server and client simulation run in your
> browser tab.

<!-- Live demo iframe — populated by CI when book is deployed to GitHub Pages -->
<div id="naia-demo-container" style="width:100%;height:600px;border:1px solid #444;border-radius:4px;overflow:hidden;">
  <iframe
    id="naia-demo"
    src="../demo/index.html"
    style="width:100%;height:100%;border:none;"
    allow="fullscreen"
    title="naia live demo">
  </iframe>
</div>

---

## What the demo shows

- Server spawning entities and replicating position updates to the client.
- Client-side prediction: move the player with arrow keys; prediction runs ahead
  and the rollback correction is visible when you introduce artificial lag.
- Priority-weighted bandwidth: entities closer to the player update faster.

---

## Running the demo locally

```sh
cd demos/basic/client/wasm_bindgen
wasm-pack build --target web
# then open index.html in your browser (requires a local HTTP server)
python3 -m http.server 8080
# open http://localhost:8080
```

---

## How the CI builds this

The GitHub Actions deploy workflow (`.github/workflows/deploy-book.yml`) builds
the `demos/basic/client/wasm_bindgen` target and copies the WASM + JS glue into
`book/book/demo/` before the GitHub Pages artifact is uploaded. The `<iframe>`
above loads from that path.
