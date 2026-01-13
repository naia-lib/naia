# One-Time Semantic Adequacy Certification (Spec → E2E Assertions)

**Owner:** Connor (source of truth for spec intent)
**Reviewer:** Chad (process + rigor)
**Executor(s):** Claude Code / Cursor agents (mechanical inspection + suggested edits)
**Scope:** One-time certification pass to confirm every spec contract is meaningfully asserted by E2E tests.
**Non-goal:** Making tests pass. Failing tests are allowed and expected.

---

## Authority

**This document is authoritative for:**
- One-time semantic adequacy certification process and completion criteria
- Policy B enforcement (every contract has at least `t1` obligation)
- Certification artifact requirements and review procedures

**Defer to:**
- `specs/README.md` for `naia-specs` tool commands (adequacy, packet, verify)
- `DEV_PROCESS.md` for ongoing SDD workflow after certification is complete
- `CLAUDE.md` for agent execution protocol (PLAN/OUTPUT convention)
- Individual specs in `specs/contracts/` for normative behavior definitions

**Note:** This is a **one-time certification plan**. After completion, this process is not added to CI. The `adequacy` tool remains available as a local dev gate.

---

## 0) Purpose

We want a single, decisive answer to this question:

> “Do we have enough E2E tests to *meaningfully assert* every behavior contract in the specs?”

This certification is a one-time “Phase A2” milestone. After it is complete, ongoing spec → E2E mapping will be manual (because spec changes should be minimal going forward), and Phase B (implementation work) proceeds from failing-but-correct tests.

---

## 1) Ground Rules

### 1.1 Specs are the source of truth
- The specs define behavior contracts.
- Tests assert those contracts.
- Implementation is adjusted later to satisfy the contracts.

### 1.2 Mechanical adequacy vs semantic adequacy
- **Mechanical adequacy**: tool-verifiable mapping (contract ids + obligation labels exist).
- **Semantic adequacy**: a competent reviewer (LLM + human) judges that the tests *actually* assert what the contract claims (not just label spam).

This plan certifies **semantic adequacy** — but uses mechanical adequacy as the scaffolding to make review deterministic and scalable.

### 1.3 We will NOT add this to CI
This is explicitly a one-time certification pass. After completion:
- `naia-specs adequacy --strict` remains useful as a **local dev gate**, but is not enforced in CI.

---

## 2) What We Already Have (Capabilities)

From the current `naia-specs` tool:

- `registry`: enumerates all contract IDs (registry is complete)
- `coverage`: confirms every contract has at least one annotated test function
- `traceability`: generates a contract ↔ test matrix
- `packet <id>`: emits a review packet (spec excerpt + tests + labels)
- `adequacy --strict`: reports missing tests / missing labels / missing obligation mappings

From the E2E harness:
- `Scenario::spec_expect("label", ...)` and `Scenario::expect_msg("label", ...)` provide labels that `naia-specs` can extract.
- Labels must have the form:
  - `contract-id: description` (contract-level)
  - `contract-id.tN: description` (obligation-level)

---

## 3) Certification Definition (What “Done” Means)

Certification is complete when ALL of the following are true:

### 3.1 Mechanical Adequacy is fully green
- `cargo run -p naia-specs -- adequacy --strict` reports:
  - Missing tests: 0
  - Missing labels: 0
  - Missing obligation mappings: 0

### 3.2 Semantic Adequacy is certified for every contract
For every contract id `X`:
- A reviewer (LLM + human spot checks) confirms:
  1) The tests referenced by the packet actually assert the intended behavior (not just a label).
  2) The test signals/observables are valid and meaningful (not implied inverses, not vacuous).
  3) If the contract contains multiple distinct claims, obligations exist and each is asserted by at least one labeled expectation.

### 3.3 Certification artifacts exist in the repo
We produce:
- A single `specs/generated/CERTIFICATION_REPORT.md`
- A deterministic list of “insufficient” contracts, with remediation notes.

---

## 4) Policy for Obligations (Uniformity)

**This certification enforces Policy B across all contracts.**

### Policy B Summary

**Every contract MUST have at least one obligation labeled `t1`.**

- Single-behavior contracts get `t1` (not optional)
- Multi-behavior contracts get `t1`, `t2`, `t3`, etc.
- No optional obligations; no "either/or"

**For complete Policy B documentation, see `specs/README.md` → "Policy B: Obligations Are Mandatory".**

This policy ensures:
- Maximum uniformity for scalable agentic review
- Clear mechanical adequacy verification (no ambiguous "NEEDS LABELS" states)
- No loopholes where unasserted sub-claims can hide

---

## 5) Required Spec + Test Conventions

**For complete conventions, see:**
- `specs/README.md` → "Policy B: Obligations Are Mandatory"
- `specs/README.md` → "Labeling Rules (Hard Constraints)"
- `specs/spec_template.md` for contract structure template

### 5.1 Spec format requirement (every contract)

Every contract MUST use this structure:
```md
### [contract-id] — Title
<normative text>
**Obligations:**
- **t1**: <testable behavior>
- **t2**: <additional behavior, if needed>
```

### 5.2 Test annotation requirement

Every test function MUST include contract annotation:
```rust
/// Contract: [contract-id], [other-id]
```

### 5.3 Obligation label requirement

Every obligation MUST have at least one labeled assertion:
```rust
scenario.spec_expect("contract-id.tN: description", || { /* assertion */ })
```

**CRITICAL:** Label string must be on same line as function call (parser requirement).

### 5.4 No lying labels

Labels must be truthful assertions:
- No "logical inverse" labels (test checks A, label claims "not A")
- No labels on empty tests or TODO stubs
- No labels that misrepresent what the test actually verifies

Failing tests are acceptable; lying labels are not.

---

## 6) Execution Plan

**For complete tool command reference, see `specs/README.md` → "Key Commands".**

This certification proceeds in 3 phases:

### Phase A0 — Tool health + determinism gate

Verify tooling is operational:
```bash
cargo run -p naia-specs -- verify --strict
cargo run -p naia-specs -- adequacy --strict
```

**Note:** If `verify --strict` fails due to test failures, proceed anyway (failing tests are allowed). We only require:
- Tooling works (commands run successfully)
- Registry and indexing work
- Test compilation is green (`cargo test -p naia-test --no-run`)

### Phase A1 — Force Policy B across all specs
Goal: every contract has obligations.

Work:
- Update every contract section in `specs/contracts/*.md` (or merged spec files) to include:
  - `**Obligations:**`
  - `- **t1**: ...`
  - and more if necessary

This is a **spec-only** pass. We do not touch tests yet.

### Phase A2 — Mechanical adequacy: obligation labels everywhere
Goal: the adequacy tool is green.

Work loop:
1) Run:
   ```bash
   cargo run -p naia-specs -- adequacy --strict
   ```
2) Take the queue top-to-bottom.
3) For each contract:
   - Generate packet:
     ```bash
     cargo run -p naia-specs -- packet <contract-id>
     ```
   - Add missing `spec_expect("contract-id.tN: ...")` labels to the correct tests.
4) Repeat until adequacy is green.

### Phase A3 — Semantic certification (LLM fan-out)
Goal: validate meaning, not just labels.

Work:
- For each contract, an LLM reviewer reads the `packet <id>` output and answers:
  - Are the tests actually asserting the obligation?
  - Are the observables correct?
  - Are obligations well-scoped and complete?
  - What changes (spec or tests) are needed to make this contract certifiably asserted?

The output is a structured rubric, per contract, written into the certification report.

---

## 7) Fan-Out Strategy (Parallelization)

We can scale Phase A3 by chunking work across agents.

### 7.1 Unit of work: “contract packet review”
Each agent is assigned a batch of contract IDs.
For each ID:
- Read packet
- Produce a rubric result:
  - PASS / FAIL
  - If FAIL: minimal edits needed (spec obligations or test observables)

### 7.2 Chunking strategy
We chunk by spec domain, because tests are organized roughly 1:1 with spec files.

Example:
- Agent 1: Connection lifecycle + transport
- Agent 2: Messaging
- Agent 3: Entity replication + scopes
- Agent 4: Time/ticks/commands
- …

### 7.3 Artifact format
Agents write results into:
- `specs/generated/CERTIFICATION_REPORT.md`

Each contract gets a small section:

```md
## contract-id — Title

Status: PASS | FAIL

Obligations:
- t1: <PASS/FAIL + why>
- t2: ...

Evidence:
- Test file(s): ...
- Labels found: ...

Fixes needed:
- (if FAIL) ...
```

## 8) Outputs (End State Artifacts)

### 8.1 Mechanical adequacy proof
- `cargo run -p naia-specs -- adequacy --strict` is green.

### 8.2 Certification report
Generated file:
- `specs/generated/CERTIFICATION_REPORT.md`

Includes:
- Summary counts
- Per-contract PASS/FAIL
- Minimal fix list for FAIL contracts
- Notes on any spec structure issues

### 8.3 Final traceability matrix
Generated file:
- `specs/generated/TRACEABILITY.md`

---

## Appendix A — Canonical Commands Cheat Sheet

```bash
# CI-grade verification (may fail due to failing tests; compilation is the key)
cargo run -p naia-specs -- verify --strict

# Mechanical adequacy (the Phase A2 gate)
cargo run -p naia-specs -- adequacy --strict

# Contract packet (the semantic review work order)
cargo run -p naia-specs -- packet <contract-id>

# Traceability matrix
cargo run -p naia-specs -- traceability

# Registry + coverage checks
cargo run -p naia-specs -- registry
cargo run -p naia-specs -- coverage

# Compile E2E tests only (no run)
cargo test -p naia-test --no-run
```

---

## Appendix B — Semantic Review Rubric (Minimal)

For each contract obligation:

- **Claim**: What is the obligation asserting?
- **Observable**: What does the test actually observe?
- **Assertion**: What exact condition does the labeled expectation check?
- **Adequacy**:
  - PASS if observable and assertion match the claim
  - FAIL if not, with minimal edits suggested