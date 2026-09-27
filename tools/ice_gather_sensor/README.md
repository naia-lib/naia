# ice_gather_sensor — manual browser-path ICE instruments

Two instruments that proved a real defect (the session offer was POSTed
before ICE gathering finished, discarding the STUN srflx candidates both
browser backends are configured to gather) and now guard the repair.

**Manual, and deliberately NOT wired into any cargo gate.** CI builds the
wasm target but runs no browser. A gated version would either skip silently
or count as coverage while running zero tests. Run by hand when the ICE
configuration or the offer path changes. If Chrome's availability in CI ever
becomes provable, wiring it in is a separate card.

## Requirements

- Headless `google-chrome` (any remote-debugging-capable Chromium).
- `node >= 22` (uses the global WebSocket; zero npm installs).
- For `check_offer.mjs` only: a live naia server plus `session_proxy.py`,
  and `python3` to run the proxy.

## Exit-code contract (both scripts)

- `0` — answerable result in the repaired shape.
- `1` — defect shape, or a rig/page failure. Read the output.
- `2` — **BLIND.** Gathering never completed, yielded nothing, Chrome is
  missing/unreachable, or the result was unparsable. UNKNOWN, never PASS.
  A positive srflx sighting is self-evidencing and is reported as `1`
  even when the gathering tail never completes; blindness only vetoes
  *absence* claims.

## 1. check_gathering.mjs — what does the browser gather?

Drives `gather_probe.html`, which configures `RTCPeerConnection` with
naia's exact ICE config (`stun:stun.l.google.com:19302`, as set in
`socket/client/src/backends/wasm_bindgen/data_channel.rs` and
`socket/client/src/backends/miniquad/naia_socket.js`), and reports the
gathered candidates plus the shape of an immediately-POSTed offer.

```
node check_gathering.mjs [path-to-gather_probe.html]
```

Recorded: host (mDNS `.local`) + srflx (`66.219.234.106` via Google STUN)
on every run; the immediately-POSTed offer shape carries zero candidate
lines, which is the defect this sensor guards against misreading.

## 2. check_offer.mjs + session_proxy.py — what does the client POST?

Drives the SHIPPED `naia_socket.js` in headless Chrome against a live
server through the logging proxy, then verdicts the captured offer body.
This is the post-repair gate: the offer must carry srflx.

```
# terminal 1: demo server (auto-accepts auth)
cargo run -p naia-server-socket-demo   # session 127.0.0.1:14191

# terminal 2: logging proxy 14201 -> 14191, captures POST bodies
python3 session_proxy.py 14201 14191 /tmp/offer.txt

# terminal 3: the gate (expect "OFFER CARRIES SRFLX", rc=0)
node check_offer.mjs \
  ../../socket/client/src/backends/miniquad/naia_socket.js \
  /tmp/offer.txt http://127.0.0.1:14201/
```

`--disable-web-security` in the driver is rig-only: the page is `file://`
and XHRs across to the proxy.

Recorded before/after on the wire (live demo server, headless Chrome):
pre-fix offer 458 bytes with zero `a=candidate:` lines; post-fix offer
carries `host` + `typ srflx`. A gathering timeout posts nothing at all
(verified: no capture file appears) and surfaces
`ice gathering did not complete within 60000ms` through the error path.

## 3. wasm_bindgen half — recipe (harness not committed)

The Rust half (`data_channel.rs`) cannot be driven without compiling a wasm
harness, so the harness lives outside this repo. The recipe that proved it:

1. Scratch crate with `naia-client-socket = { path = ..., features =
   ["wbindgen"] }`, exporting `start(url)` (calls
   `Socket::connect_with_auth`) and `poll()` over the identity receiver.
2. `cargo build --target wasm32-unknown-unknown`, then glue with a
   `wasm-bindgen` CLI that **matches the lib version** (a mismatch fails at
   `table.grow` during init; 0.2.129 proved good).
3. Serve over http, drive in headless Chrome against the proxy rig above,
   verdict the captured offer the same way.

Recorded: pre-fix candidate-less offer; post-fix `host` + `typ srflx`
(`66.219.234.106`).

## In-repo structural cover

`socket/client/src/lib.rs::miniquad_js_bridge_host_oracle` pins the repair
without a browser (host-executable, runs in `cargo test`):
`the_offer_post_waits_for_gathering_complete` (single send site behind the
`iceGatheringState` gate + loud timeout) and
`the_wasm_backend_gates_its_offer_the_same_way` (both halves report the
identical timeout message). Both were shown red against the pre-fix sources.
