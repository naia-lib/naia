# naia release process

**Trunk for in-flight work:** `dev`.
**Default GitHub branch:** `main`.
**`main` is updated only at release time.**

This is naia's branching policy, in continuous use since the repo's creation
(2019). It was briefly violated 2026-05-06 → 2026-05-07; see
`BRANCH_REWIND_2026-05-07.md`.

---

## Why this matters

`main` represents what's published on crates.io. New consumers cloning the repo
land on `main` (it's the default branch). Trapping in-flight work on `main`
ships unreleased changes to anyone cloning, and breaks the convention this
repo has used since 2019.

---

## Cutting a release

1. Verify `dev` is green:
   ```bash
   git checkout dev
   RUSTFLAGS="-D warnings" cargo build --workspace --all-targets
   cargo test --workspace --all-targets
   cargo run -p naia-npa -- run -p test/specs/resolved_plan.json -o test/specs/run_report.json
   ```
2. Fast-forward merge `dev` → `main`:
   ```bash
   git checkout main
   git merge --ff-only dev
   ```
   If `--ff-only` fails, do **not** use a non-FF merge. `main` should never
   have commits that aren't on `dev`. If FF is rejected, investigate why
   `main` moved before proceeding.
3. Tag the release:
   ```bash
   git tag v0.25.0
   ```
4. Push:
   ```bash
   git push origin main --tags
   ```
5. Publish crates in dependency order (existing crate-publish process applies).
6. Switch back to `dev` for the next cycle:
   ```bash
   git checkout dev
   ```
   Same `dev` branch keeps being used; no rename needed.

---

## Hard rules for agents

- **Never run `git checkout main`** to make commits. The only `git checkout main`
  in the workflow is step 2 of "Cutting a release" above.
- **Never run `git merge release-* main` or `git merge dev main`** outside
  step 2 of "Cutting a release" above.
- **Never run `git push origin main`** outside step 4 of "Cutting a release"
  above (i.e. without an accompanying release tag).
- If a plan doc tells you to "commit on main", **treat that as a likely bug**
  in the plan. Surface it to the operator before executing. This is exactly
  how the 2026-05-06 incident happened.

---

## Recovery: how to detect a recurrence

```bash
# main should always be at exactly one of: a release tag, or an ancestor of dev
git merge-base --is-ancestor main dev && echo "OK: main is ancestor of dev" || echo "DRIFT"
git tag --points-at main          # should print v0.X.Y if main is at a release
git log main..dev --oneline | wc -l   # commits ahead = unreleased work waiting
git log dev..main --oneline | wc -l   # MUST be 0 — main has nothing dev doesn't
```

If `dev..main` is non-zero: someone committed on main outside the release process.
Stop, investigate, and rewind per `BRANCH_REWIND_2026-05-07.md` if needed.
