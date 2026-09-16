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

## Sibling consumers (in-family producers)

External consumers take naia from crates.io — that is, from `main`. A
co-developed sibling that needs dev-only surface instead checks naia out from
the pin its own repo declares, e.g. cyberlith's `.github/ci-producers.json`:

```json
"naia": {
  "repository": "naia-lib/naia",
  "checkout_revision": "dev",
  "revisions": { "dev": { "ref": "refs/heads/dev", "sha": "<sha>" } }
}
```

Pinning `dev` is the intended mode for those siblings; there is no expectation
that every consumer follows `main`.

**A naia ref advance is visible to a sibling consumer only once the consumer's
own `ci-producers.json` advances.** Until then the consumer's tree and naia's
tip are two different bases, and a checkout that mixes them fails *by
construction* — not because either side is broken.

**And what a consumer checks out is the lock's `sha`, not the branch its `ref`
string names.** In cyberlith, `tools/ci/verify-producer-lock.py:154` compares
`git rev-parse HEAD` against the locked sha and fails closed
(`require(head == selected["sha"], …)`), while the `ref` field is only
syntax-checked and then never used to resolve anything; and
`.github/workflows/client-boot.yml:96,107` selects
`jq -er '.producers.naia.revisions.dev.sha'` and hands it to `actions/checkout`
as `ref:`. Nothing reads `refs/heads/dev`. So a naia ref advance on its own
cannot change what a sibling builds, and a sibling whose checkout drifted fails
loudly (`HEAD <x> != locked <y>`) rather than silently.

**Worked example (2026-09-16).** Cyberlith's `main` tree declared naia
`f1c802d8343b3ae62a581c17774028fbb91b4581` (2026-09-06). The `WireSchema` trait
does not exist at that ref at all:

```bash
git grep -l WireSchema f1c802d8 | wc -l   #  0
git grep -l WireSchema d82d3d99 | wc -l   # 30  (cyberlith dev's declared pin)
git grep -l WireSchema 0ef641ed | wc -l   # 32  (naia dev tip)
```

A session that measured that tree with naia checked out at `0ef641ed` reported
`error[E0277]: the trait bound ChatDisplayName: WireSchema is not satisfied` on
every row and concluded the tree could not be measured. It could: the producer
checkout had moved and the consumer's declared pin had not.

**The cheap check is presence, not a build.** A symbol absent from the pin a
tree declares cannot be unsatisfied at that pin, and `git grep -l <Symbol> <pin>`
answers that in one command, without compiling anything.

When a release carries a consumer-visible surface change — a new trait, a new
bound, or a derive that consumers must now satisfy — name it in the release
notes (`CHANGELOG.md`) as a consumer-visible surface change, with the ref that
introduces it, so a consumer can run that presence check against its own pin.

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
