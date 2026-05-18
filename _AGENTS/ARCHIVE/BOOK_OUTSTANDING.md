# Naia mdBook — Outstanding Work Items

All Phase 1–3 work (scaffold, content, Bevy-first restructure, quality audit) is complete
and committed on `dev`. The items below are the remaining phases.

---

## Manual prerequisite (do once after merging to main)

**Enable GitHub Pages in the repo settings:**

1. Go to `https://github.com/naia-lib/naia` → Settings → Pages
2. Under **Source**, select **GitHub Actions** (not "Deploy from a branch")
3. Save. The next push to `main` that touches `book/**` will trigger
   `.github/workflows/deploy-book.yml` and publish to
   `https://naia-lib.github.io/naia/`.

The deploy workflow is already committed at `.github/workflows/deploy-book.yml`.

---

## Phase 4 — Live In-Browser Demo

**Goal:** Embed a playable demo at `book/src/demo/live.md` so readers can try naia
without cloning anything.

**Approach (zero-backend):**

1. Restructure `demos/basic/client/` to add a `transport_local` mode that runs both
   client and server inside a single WASM binary (same thread, `transport_local`
   socket, no network). This avoids needing a live backend.

2. Build the WASM artifact in CI:
   ```yaml
   # inside deploy-book.yml, before mdbook build
   - name: Build demo WASM
     run: |
       cargo build -p naia-demo-basic-client \
         --target wasm32-unknown-unknown \
         --no-default-features --features transport_local
       wasm-bindgen target/wasm32-unknown-unknown/debug/naia_demo_basic_client.wasm \
         --out-dir book/src/demo/wasm --target web
   ```

3. Add a small `index.html` wrapper in `demos/basic/client/wasm_bindgen/` that
   imports the WASM and renders to a `<canvas>`.

4. Embed in `book/src/demo/live.md`:
   ```html
   <iframe src="wasm/index.html" width="800" height="600"
           style="border:none; border-radius:8px;"></iframe>
   ```

**Acceptance:** visiting the live book URL shows a running demo without any server setup.

---

## Phase 5 — Polish

### 5A — Link checker in CI

Add `mdbook-linkcheck` to the deploy workflow so broken internal links fail the build:

```yaml
- name: Install mdbook-linkcheck
  run: cargo install mdbook-linkcheck --version 0.7.7 --locked

- name: Check book links
  run: mdbook-linkcheck book/
```

Add to `book/book.toml`:
```toml
[output.linkcheck]
```

Note: `mdbook-admonish` was removed, so there is no longer any version-conflict risk
when adding additional preprocessors.

### 5B — README badge

After the GitHub Pages URL is live, add a badge to the top of `README.md`:

```markdown
[![Book](https://img.shields.io/badge/book-naia--lib.github.io-blue)](https://naia-lib.github.io/naia/)
```

Place it alongside the existing crates.io / docs.rs / license badges.

### 5C — arewegameyet.rs submission

Submit naia to [arewegameyet.rs](https://arewegameyet.rs/):

1. Fork `https://github.com/rust-gamedev/arewegameyet`
2. Add naia to `data/crates.toml` under the networking section
3. Open a PR

This is a manual, one-time step requiring a GitHub login — not automatable in CI.

---

## Notes

- The book build is `mdbook build book/` from the repo root.
- `mdbook-mermaid` and `mdbook-pagetoc` are the only preprocessors currently active.
- All 30 source files under `book/src/` are complete; no placeholder stubs remain.
