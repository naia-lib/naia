# naia — agent entrypoint

Spec files: `test/specs/contracts/` (start at `test/specs/contracts/00_common.spec.md`); design docs in `book/` and `docs/`.

Application boundary (`00_common.spec.md` §7, Connor 2026-10-27): naia owns connection lifecycle, transport, typed-message delivery and entity replication. Application state and policy belong to the application protocol and its consumer — no game names, fixed player counts or gameplay deadlines in public naia APIs. Check each new public type, fixed limit and state transition against that rule before landing on `dev`.
