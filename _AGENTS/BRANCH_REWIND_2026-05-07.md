# Branch rewind runbook — 2026-05-07

**Status:** PLANNED. Awaiting Connor go-ahead at the marked STOP points.
**Operator:** twin-Claude.
**Why this exists:** between 2026-05-06 00:30 -0600 and 2026-05-07 11:12 -0600, in-flight SDD-migration / Track-2 / quality-debt work was committed directly on `main` instead of on a release branch. This violates naia's 6-year release-branch convention. This document is the bulletproof runbook to rewind `main` back to its pre-merge state and put all the recent work on a new `dev` branch, without losing a single commit, file, or local artifact.

---

## 1. Critical SHA registry — protect these at all costs

| Label | SHA | Date (authored) | What it is | Where it lives now | Where it must end up |
|---|---|---|---|---|---|
| `MAIN_NEW_TIP` | `8ffea924` | 2026-05-07 11:12 | Q3 split — current `main` and `origin/main` tip | `main`, `origin/main` | `dev`, `origin/dev` |
| `RELEASE_E_TIP` | `84e9861f` | 2026-05-05 23:10 | T1.3 — old `release-0.25.0-e` tip; merge base | local `release-0.25.0-e` (origin deleted by Connor) | reachable from `dev` (already is — ancestor of `MAIN_NEW_TIP`) |
| `MAIN_OLD_TIP` | `4b646a1f` | 2024-12-27 13:14 | Pre-merge `main` (naia_client_socket 0.24.1) | reachable from `MAIN_NEW_TIP` | `main`, `origin/main` |
**Disposable (Connor confirmed 2026-05-07: not relevant to this repo any more — all valuable work was in release-0.25.0-e before the merge):**
- `bevy-demo-entity-relations` (`567653e5`) — 49 unique local commits, discarded
- `release-0.25.0-b` (`19c7d487`) — 4 unique commits, discarded
- `release-0.25.0-d2` (`fef7e01a`) — 1 unique commit, discarded

**Subsumed (safe to delete, fully ancestor of `main`):**
- `release-0.25.0-c` (`b20b48c3`)
- `release-0.25.0-d` (`e02a82ed`)
- `release-0.25.0-e` (`84e9861f`)

**Out of scope (don't touch):** the contributor-fork remotes `anellie` (404), `charles`, `mvlabat`. Operations target ONLY `origin`.

---

## 2. Risk register

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| R1 | Force-push race: someone pushes to origin/main between our check and our push | LOW (no contributors) | Use `git push --force-with-lease=main:8ffea924 origin main` — aborts if origin/main isn't exactly `8ffea924` |
| R2 | Force-push rejected by GitHub branch protection | LOW (Connor will check) | Connor confirms in §6 pre-flight that no rule blocks force-push to main |
| R3 | Working tree has uncommitted changes when we start | LOW | Step P0.1 hard-checks; abort if dirty |
| R4 | Push to wrong remote (anellie/charles/mvlabat) | LOW | Every push command names `origin` explicitly |
| R5 | We become detached during rewind and lose orientation | MED | Always `git checkout dev` BEFORE running `git branch -f main ...` so HEAD never points at the moving ref |
| R6 | Local clone corrupts or is lost mid-operation | LOW | Step P0.0 makes a full filesystem-level backup clone (`naia-backup-2026-05-07`) outside this directory before any edits |
| R7 | Reflog GCs the rewound commits | NONE | reflog default 90 days; ALSO origin/dev preserves them; ALSO the backup clone preserves them |
| R8 | ~~bevy-demo-entity-relations 49 commits~~ — DISPOSABLE per Connor 2026-05-07 | NONE | Backup clone P0.0 retains them in case Connor changes mind within ~90 days |
| R9 | ~~release-0.25.0-b/d2 unique commits~~ — DISPOSABLE per Connor 2026-05-07 | NONE | Backup clone P0.0 retains them; reflog preserves for ~90 days |
| R10 | Pre-push hook fails on the force-push (cargo check or wasm check breaks) | LOW (we've been pushing successfully through Q1-Q3) | Hook runs against the working tree (which is `dev`'s tree, known clean) — should pass. If it fails, fix the underlying issue before retrying; do NOT bypass with `--no-verify` |
| R11 | Plan doc updates (`SDD_MIGRATION_PLAN.md`, `SDD_QUALITY_DEBT_PLAN.md`) accidentally edited on rewound `main` | NONE if we order correctly | Doc updates happen on `dev` AFTER the rewind, not before |
| R12 | Tags on rewound commits get orphaned | NONE | Last tag is `v0.24.0`, far before `MAIN_OLD_TIP`. No tags between `MAIN_OLD_TIP` and `MAIN_NEW_TIP`. Verified |
| R13 | Submodules out of sync after rewind | NONE | `git submodule status` returned empty — naia has no submodules |
| R14 | Multiple worktrees pointing at moving refs | NONE | `git worktree list` shows only the primary worktree |
| R15 | A future Claude session re-runs the same mistake | MED | Step P4 creates an auto-memory feedback rule + repo-side `_AGENTS/RELEASE_PROCESS.md` so the dev-trunk policy is impossible to miss |
| R16 | Dependabot or another origin/* branch has unique commits we don't know about | LOW | `origin/dependabot/cargo/syn-2.0.18` has 1 unique commit (ancient auto-PR); leave alone — not our concern, Connor can prune via GitHub UI later |
| R17 | `cargo publish` accidentally triggers between phases | NONE | We don't run cargo publish anywhere in this runbook. crates.io stays at v0.24.0 |
| R18 | A long-running background task (criterion benches from Q0) writes files into the working tree mid-operation | LOW | Step P0.2 explicitly checks for and stops any background processes before proceeding |

---

## 3. The runbook

Every phase ends with a verification check. After each phase we should be **strictly safer than before** — never more exposed, never having lost optionality.

### Phase 0 — pre-flight (zero risk, all reads + one filesystem backup)

#### P0.0 — Filesystem-level backup (the ultimate safety net)

```bash
cd /home/connor/Work/specops
cp -a naia naia-backup-2026-05-07
ls -la naia-backup-2026-05-07/.git | head
```

Rationale: a complete byte-for-byte copy of `naia` (working tree + `.git`) at a sibling path. If anything else in this runbook goes wrong, `rm -rf naia && mv naia-backup-2026-05-07 naia` restores the exact pre-runbook state. This is independent of git's reflog and origin's state.

**Verification:** `cd naia-backup-2026-05-07 && git log -1 --oneline` shows `8ffea924`.

#### P0.1 — Working tree hygiene

```bash
cd /home/connor/Work/specops/naia
git status --porcelain | head    # MUST be empty (no untracked, no modified)
git rev-parse HEAD                # MUST equal 8ffea924…
git rev-parse --abbrev-ref HEAD   # MUST equal main
```

Abort if any of these don't match. If untracked files exist that shouldn't, `git clean -nd` first to confirm what they are; do NOT `-f` clean blindly.

#### P0.2 — Stop background tasks

Stop any criterion bench processes (Q0 in the prior plan):

```bash
ps -ef | grep -E "criterion|cargo bench|cargo run -p naia" | grep -v grep
# kill any matches; the bench won't be needed for the rewind
```

#### P0.3 — Sync remote view

```bash
git fetch --all --prune
git log main..origin/main --oneline   # MUST be empty
git log origin/main..main --oneline   # MUST be empty
```

If these are not both empty, abort and reconcile first.

#### P0.4 — Verify the SHA registry

```bash
git log release-0.25.0-e..main --oneline | wc -l   # MUST print 51 (NEW SDD commits since FF merge)
git log 4b646a1f..main --oneline | wc -l            # MUST print 632 (51 NEW + 581 from release-0.25.0-e history)
git rev-parse 84e9861f^{commit}                     # MUST not error
git merge-base --is-ancestor 84e9861f main && echo OK || echo FAIL  # OK
git merge-base --is-ancestor 4b646a1f main && echo OK || echo FAIL  # OK
```

#### P0.5 — Inventory of unique-commit branches

```bash
git log bevy-demo-entity-relations --not main --oneline | wc -l    # MUST print 49
git log release-0.25.0-b --not main --oneline | wc -l               # MUST print 4
git log release-0.25.0-d2 --not main --oneline | wc -l              # MUST print 1
git log release-0.25.0-c --not main --oneline | wc -l               # MUST print 0
git log release-0.25.0-d --not main --oneline | wc -l               # MUST print 0
git log release-0.25.0-e --not main --oneline | wc -l               # MUST print 0
```

If any of these don't match, STOP and re-investigate before proceeding.

**STOP — Connor confirms verification output before Phase 1.**

---

### Phase 1 — create `dev` from current `main` tip (zero risk, additive only)

#### P1.1 — Create `dev` locally

```bash
git branch dev main
git rev-parse dev                # MUST equal 8ffea924…
```

#### P1.2 — Push `dev` to origin

```bash
git push -u origin dev
```

This may trigger the pre-push hook. Expected: hook passes (we've been pushing clean).

#### P1.3 — Verify origin/dev

```bash
git fetch origin
git rev-parse origin/dev          # MUST equal 8ffea924…
git log origin/main..origin/dev   # MUST be empty (they're the same commit at this moment)
git log origin/dev..origin/main   # MUST be empty
```

After Phase 1: `origin/dev` and `origin/main` both point at `8ffea924`. Every commit is durable on origin under TWO names.

**STOP — Connor confirms `origin/dev` exists at `8ffea924` (visible on GitHub). Explicit "go" required before Phase 2.**

---

### Phase 2 — rewind `main` (the destructive step)

#### P2.1 — Switch HEAD to `dev` first (mitigates R5)

```bash
git checkout dev
git rev-parse --abbrev-ref HEAD   # MUST equal dev
```

After this point, `main` is no longer the checked-out branch. Moving its ref is a pure pointer update with no working-tree effect.

#### P2.2 — Move local `main` to pre-merge SHA

```bash
git branch -f main 4b646a1f
git rev-parse main                # MUST equal 4b646a1f…
git log main -1 --oneline         # MUST show "Update naia_client_socket to version 0.24.1 - Update IDEA run configurations"
```

#### P2.3 — Force-push `main` to origin (with lease guard)

```bash
git push --force-with-lease=main:8ffea924 origin main
```

The `--force-with-lease` form: the push proceeds only if `origin/main` is currently exactly `8ffea924`. If anyone (including ourselves from another clone) has pushed since, the push is rejected and nothing changes. This is the safe form of `--force`.

Expected: pre-push hook runs against the dev working tree (clean, builds), passes; force-push lands.

If the pre-push hook fails: do NOT use `--no-verify`. Investigate, fix the cause, retry. Common causes: a cargo dep was yanked, a wasm check broke. Both are fixable on `dev` first; commit to `dev`, push `dev`, then re-attempt the force-push.

#### P2.4 — Verify post-force-push state

```bash
git fetch origin
git rev-parse origin/main         # MUST equal 4b646a1f…
git rev-parse origin/dev          # MUST equal 8ffea924…
git log origin/main..origin/dev | wc -l   # MUST print 51
git log origin/dev..origin/main           # MUST be empty
```

After Phase 2: `origin/main` is rewound; `origin/dev` carries all 51 commits; HEAD is on `dev`.

---

### Phase 3 — clean up stale local branches (simplified per Connor 2026-05-07)

Connor confirmed all six stale branches are disposable: `bevy-demo-entity-relations`, `release-0.25.0-b/c/d/d2/e`. Backup clone (P0.0) retains them in case of regret within ~90 days; reflog also retains.

#### P3.1 — Delete all stale local branches

```bash
git branch -D bevy-demo-entity-relations \
  release-0.25.0-b release-0.25.0-c release-0.25.0-d release-0.25.0-d2 release-0.25.0-e
```

The `-D` flag forces deletion even when branches aren't merged into HEAD. Required because three of them have unique commits relative to `dev`.

#### P3.2 — Prune any cached origin refs

```bash
git fetch --prune origin
```

#### P3.3 — Verification

```bash
git for-each-ref refs/heads --format='%(refname:short) %(objectname:short)'
# Should show EXACTLY: dev, main
```

---

### Phase 4 — doc + memory updates (zero risk, all on `dev`)

All commits in this phase land on `dev` (and only `dev` — `main` stays at `4b646a1f`).

#### P4.1 — Update `_AGENTS/SDD_MIGRATION_PLAN.md`

- Find every reference to "commit on `main`", "push to `main`", "merged to main", and rewrite as `dev`.
- Add at the very top (after the Status line):
  > **Branching policy (re-established 2026-05-07):** all in-flight work lives on `dev`. `main` is touched only at release time (merge `dev` → `main` + tag + `cargo publish`). The wholesale move of in-flight work onto `main` between 2026-05-06 and 2026-05-07 was a process error caught and corrected on 2026-05-07; see `BRANCH_REWIND_2026-05-07.md`.

#### P4.2 — Update `_AGENTS/SDD_QUALITY_DEBT_PLAN.md`

- "**Gate per phase:** ... commit + push to `main`" → "...commit + push to `dev`"
- Same in §6.2.4 verification step.
- Same banner addition as P4.1.

#### P4.3 — Create `_AGENTS/RELEASE_PROCESS.md`

New short doc, ≤ 40 lines, single source of truth for the branching policy:

```markdown
# naia release process

**Trunk for in-flight work:** `dev`.
**Default GitHub branch:** `main` (Connor's contributors-disabled repo policy).
**`main` is updated only at release time.**

## Cutting a release

1. Verify `dev` is green: `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets`,
   `cargo run -p naia_npa -- run -p test/specs/resolved_plan.json`, integration tests.
2. Merge `dev` → `main` as a fast-forward: `git checkout main && git merge --ff-only dev`.
   If FF is impossible, do NOT use a non-FF merge — abort and figure out why main moved.
3. Tag: `git tag v0.25.0`.
4. Push: `git push origin main --tags`.
5. Publish crates in dependency order (existing crate-publish process applies).
6. After release, continue work on `dev`. New cycles do not require a new branch name.

## Why this matters

`main` represents what's published on crates.io. New consumers cloning the repo
land on `main` (it's the default branch). Trapping in-flight work on `main`
ships unreleased changes to anyone cloning, and breaks the 6-year convention
this repo has used since 2019.

The 2026-05-06 incident: an autonomous Claude session, instructed by the SDD
migration plan to "commit on main", ran a fast-forward merge of release-0.25.0-e
into main and committed 51 in-flight commits there. Caught and rewound on
2026-05-07. See BRANCH_REWIND_2026-05-07.md.

## Hard rules for agents

- **Never run `git checkout main`** for the purpose of committing.
- **Never run `git merge release-* main` or `git merge dev main`** outside
  step 2 of "Cutting a release" above.
- If a plan doc tells you to "commit on main", treat that as a likely bug
  in the plan — surface it to the operator before executing.
```

#### P4.4 — Update `_AGENTS/CODEBASE_AUDIT.md`

`grep -n '\bmain\b' _AGENTS/CODEBASE_AUDIT.md` and rewrite git-branch references (vs. `fn main()` references). Spot-check; minor.

#### P4.5 — Auto-memory entry (outside the repo)

File: `/home/connor/.claude/projects/-home-connor-Work-specops/memory/feedback_naia_branching_policy.md`

```markdown
---
name: naia uses dev-trunk + release-branch model — never commit on main
description: branching policy for naia repo; main is touched only at tag time, all in-flight work lives on dev
type: feedback
---

naia (and historically Connor's repos) uses a release-branch model: in-flight
work lives on `dev` (or a release branch), `main` is updated only when a
release is cut and crates are published.

**Why:** Connor's been using this for ~6 years. crates.io publishes lag git
pushes; main needs to match the last published state so consumers cloning at
HEAD don't get unreleased changes. Default GitHub branch is `main`, so
clone-at-HEAD lands on the published state.

**How to apply:** for naia, default to `dev` for any commits. Never run
`git checkout main` to commit on it. If a plan doc says "commit on main",
flag it as a likely bug — that's how the 2026-05-06 FF-merge incident
happened (autonomous Claude session followed a plan instruction literally,
fast-forwarded release-0.25.0-e into main, committed 51 in-flight commits).
Caught and rewound 2026-05-07.

The `_AGENTS/RELEASE_PROCESS.md` in the naia repo is the canonical reference.
```

Then add to `MEMORY.md`:
```
- [naia uses dev-trunk + release-branch model](feedback_naia_branching_policy.md) — never commit on main; main is touched only at tag time
```

#### P4.6 — Commit + push doc updates to `dev`

```bash
git checkout dev   # confirm we're on dev
git add _AGENTS/SDD_MIGRATION_PLAN.md _AGENTS/SDD_QUALITY_DEBT_PLAN.md \
        _AGENTS/RELEASE_PROCESS.md _AGENTS/CODEBASE_AUDIT.md \
        _AGENTS/BRANCH_REWIND_2026-05-07.md
git commit -m "docs: dev-trunk branching policy + branch-rewind runbook"
git push origin dev
```

---

### Phase 5 — final verification

```bash
git rev-parse origin/main                         # 4b646a1f…
git rev-parse origin/dev                          # 8ffea924… or later (after P4 commit)
git log origin/main..origin/dev --oneline | wc -l # 51 + N (N=doc commits from P4)
git log origin/dev..origin/main                   # empty
git rev-parse --abbrev-ref HEAD                   # dev
git for-each-ref refs/heads                       # only dev, main (+ bevy-demo if A2)
git tag --list 'archive/*'                        # archive/release-0.25.0-b, archive/release-0.25.0-d2
RUSTFLAGS="-D warnings" cargo build --workspace --all-targets   # clean
cargo run -p naia_npa -- run -p test/specs/resolved_plan.json -o test/specs/run_report.json
                                                   # 172/172 pass
```

If every line above checks: **the rewind is complete and bulletproof.**

---

## 4. Recovery runbook (if something goes wrong)

### Recovery R-1 — pre-Phase-1 abort

If anything in Phase 0 fails verification: stop. The repo is unchanged. No recovery needed.

### Recovery R-2 — Phase 1 push fails

If `git push -u origin dev` fails (e.g., pre-push hook blocks): no force-push has happened yet, `main` is unchanged. Fix the underlying issue (cargo error, network), retry the push. If `dev` exists locally but not on origin, you can `git branch -D dev` to remove it and re-create it after the fix.

### Recovery R-3 — Phase 2 force-push aborts via lease guard

`--force-with-lease` rejected the push because origin/main moved. Re-run P0.3 to inspect what's on origin/main now, decide whether to incorporate that change, and retry. Local state is not damaged.

### Recovery R-4 — Phase 2 succeeds but post-verification fails

Unlikely but possible if a hook or hosting-side hook rewrote things. Restore from the P0.0 backup:
```bash
cd /home/connor/Work/specops
mv naia naia-broken-$(date +%s)
mv naia-backup-2026-05-07 naia
cd naia
# fast-forward origin to recover origin/main if needed:
git push --force-with-lease origin main:main   # restores origin/main to 8ffea924
git push origin :dev                            # deletes origin/dev if it exists
```

### Recovery R-5 — total local clone corruption

Use the P0.0 filesystem backup. If that's also gone, the SHA registry above + origin/dev (which we pushed in Phase 1) are sufficient to reconstruct: clone fresh from origin and `git checkout origin/dev`.

### Recovery R-6 — recover individual commits from reflog

```bash
git reflog HEAD | grep <sha-prefix>
git reset --hard <reflog-entry>
```

Reflog default retention: 90 days for reachable, 30 days for unreachable. The 51 commits on `dev` are reachable via origin/dev so this isn't needed for them; useful only if `bevy-demo-entity-relations` got accidentally deleted before P3.A.

---

## 5. Things only Connor can confirm (pre-flight)

Before P0.0, Connor should personally confirm in the GitHub UI (or git CLI if he prefers):

- [x] **GitHub branch protection on `main`:** force-push enabled by Connor 2026-05-07.
- [x] **Stale-branch disposal:** Connor confirmed 2026-05-07 that `bevy-demo-entity-relations`, `release-0.25.0-b`, and `release-0.25.0-d2` are not relevant to the repo any more; all valuable commits were in `release-0.25.0-e` before the merge. No archiving needed; backup clone (P0.0) retains them as a safety net.
- [ ] (No other manual checks required.)

---

## 6. Execution log (filled in as we go)

| Phase | Status | Timestamp | SHA / Notes |
|---|---|---|---|
| P0.0 backup | ✅ done | 2026-05-07 11:53 | `naia-backup-2026-05-07/` (41 GB), tip = 8ffea924 |
| P0.1 hygiene | ✅ done | 2026-05-07 11:55 | clean tree (only untracked: this doc), HEAD on main at 8ffea924 |
| P0.2 stop tasks | ✅ done | 2026-05-07 11:55 | no benches running |
| P0.3 sync | ✅ done | 2026-05-07 11:55 | origin/main == main; anellie remote 404 (dead fork, ignored) |
| P0.4 SHA verify | ✅ done | 2026-05-07 11:55 | release-0.25.0-e..main = 51 ✓, ancestors check ✓ |
| P0.5 unique-commit inventory | ✅ done | 2026-05-07 11:55 | bevy-demo +49, release-b +4, release-d2 +1 (all disposable per Connor) |
| P1.1 dev local | ✅ done | 2026-05-07 (Connor "go") | `git branch dev main`; dev = 8ffea924 |
| P1.2 dev push | ✅ done | 2026-05-07 | pre-push hook passed; origin/dev = 8ffea924 |
| P1.3 dev verify | ✅ done | 2026-05-07 | origin/main == origin/dev == 8ffea924 |
| **STOP — Connor go-ahead** | ✅ approved | 2026-05-07 | "Go to Phase 2, be careful here!" |
| P2.1 checkout dev | ✅ done | 2026-05-07 | HEAD on dev (8ffea924) |
| P2.2 move main | ✅ done | 2026-05-07 | local main → 4b646a1f |
| P2.3 force-push main | ✅ done | 2026-05-07 | `--force-with-lease=main:8ffea924` succeeded; admin override of branch protection |
| P2.4 verify | ✅ done | 2026-05-07 | origin/main = 4b646a1f, origin/dev = 8ffea924 |
| P3.1 delete stale branches | ✅ done | 2026-05-07 | bevy-demo + release-0.25.0-{b,c,d,d2,e} deleted locally |
| P3.2 prune origin | ✅ done | 2026-05-07 | `--prune` ran; only `dev`, `main`, `dependabot/...`, `release-0.25.0-e` (Connor handling) remained on origin |
| P3.3 verify | ✅ done | 2026-05-07 | local refs: only `dev` and `main` |
| P4 doc updates | ✅ done | 2026-05-07 (commit 83a7f83c) | SDD plans updated; RELEASE_PROCESS.md + BRANCH_REWIND_2026-05-07.md committed to dev |
| P5 final verification | ✅ done | 2026-05-07 | NPA 172/172, build clean, dev..main = 0 (drift gate green) |
| Connor — delete origin/release-0.25.0-e | confirmed handling | 2026-05-07 | "I will remove release-0.25.0-e myself on GH" |

---

## 7. After completion: what changes for future agents

- Memory file `feedback_naia_branching_policy.md` (P4.5) means future Claude sessions starting fresh will load this rule into context automatically (`MEMORY.md` is auto-loaded).
- `_AGENTS/RELEASE_PROCESS.md` is the in-repo canonical reference, citable from plan docs.
- Plan docs (SDD_*) are corrected to say "push to `dev`".
- The `BRANCH_REWIND_2026-05-07.md` (this doc) becomes the historical record of what happened, why, and how it was fixed.
