# webrtc-unreliable-client — agent notes

Spec files: `README.md` (crate purpose, runtime/executor contract) and
`siblings.lock` (exact naia `dev` SHA this revision builds against; the
dev-dependency on `naia-server-socket` that `tests/loopback.rs` uses).

Sibling law (Connor 2026-10-22): consumers use this repo only by
`path = "../webrtc-unreliable-client/..."`, never `git`/`rev`/`tag`.
Copying this provider into a consumer is not wanted (Usher ruling on
nydus-7zv, 2026-10-29); consume it through the ordinary relative sibling
path plus `siblings.lock`.

Merge gate: `tests/loopback.rs` runs this client against a real
`naia-server-socket` server; keep it green. Behavior changes land with
their spec note in the same commit.
