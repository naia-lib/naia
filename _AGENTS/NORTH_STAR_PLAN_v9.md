# North Star Plan

## The Authoritative Specification System for Naia (Powered by Namako)

---

## Table of Contents

1. [Executive Summary](#part-1-executive-summary)
2. [Architectural Philosophy: The Bulletproof Core](#part-2-architectural-philosophy-the-bulletproof-core)
3. [The Six Goals](#part-3-the-six-goals)
4. [System Architecture](#part-4-system-architecture)
5. [The Namako Engine & Resolution](#part-5-the-namako-engine--resolution)
6. [The ID Scheme & Stability](#part-6-the-id-scheme--stability)
7. [The Spec Surface & Canonicalization](#part-7-the-spec-surface--canonicalization)
8. [The Eight Invariants](#part-8-the-eight-invariants)
9. [The Namako Project Adapter Protocol (NPAP)](#part-9-the-namako-project-adapter-protocol-npap)
10. [Runtime & Execution: The Plan-Driven Model](#part-10-runtime--execution-the-plan-driven-model)
11. [Certification & Integrity Workflow](#part-11-certification--integrity-workflow)
12. [The Namako CLI](#part-12-the-namako-cli)
13. [Naia Integration](#part-13-naia-integration)
14. [Migration Plan](#part-14-migration-plan)
15. [Definition of Done](#part-15-definition-of-done)
16. [Appendix A: The SDD loop UX](#part-16-the-ai-assisted-spec-driven-development-loop)

---

## Part 1: Executive Summary

This document specifies the **definitive, authoritative spec-driven development system** for Naia, built upon **Namako**. The v9 revision introduces **AST-normal-form hashing**, **signature enforcement**, and **plan header verification** to make the system **bulletproof** against drift and implementation divergence.

```
spec → engine(resolve) → plan → adapter(execute) → evidence → verify
```

### The Core Insight: Plan-Driven Integrity

Drift occurs when the linter thinks a step matches Binding A, but the runtime matches Binding B (or matches loosely). **Namako v9 eliminates this class of error entirely.**

The **Namako Engine** is the sole source of matching logic. It resolves every step into a **Resolved Execution Plan**. The project adapter is **structurally forbidden** from performing text matching; it simply executes the Binding IDs dictated by the Engine.

### The Integrity Model

A scenario is **Integrity-Certified** when we possess **hash-based integrity evidence** proving that:
1.  **The Spec is Canonical:** The input `.feature` is hashed strictly from its AST normal form (`feature_ast_hash`).
2.  **The Plan is Authoritative:** The `resolved_plan_hash` matches the baseline, proving exact step→binding resolution, arguments, and binding signatures.
3.  **The Execution was Faithful:** The adapter executed exactly the Binding IDs and payloads specified in the plan.
4.  **No Drift Exists:** The candidate certification tuple allows zero bits of deviation from the committed baseline.

---

## Part 2: Architectural Philosophy: The Bulletproof Core

### 2.1 The "Engine resolves, Adapter obeys" Rule

In traditional tools, the test runner reads regexes and matches strings at runtime. **This is banned in Namako.**

*   **Namako Engine (The Brain):** Parses Gherkin -> Matches text or regex -> Selects Binding ID -> Extracts Captures -> Output: **Resolved Execution Plan**.
*   **Project Adapter (The Muscle):** Receives Plan -> Looks up Binding ID -> Invokes Code with provided Captures.
    *   Adapter MUST execute steps **by Binding ID only** using a direct lookup.
    *   Adapter MUST NOT derive binding choice from `step_text` or matching logic.

**Benefit:** The adapter cannot "accidentally" run the wrong step or misinterpret a parameter. If the Engine says "Run binding `foo-01` with arg `50`", the Adapter does exactly that.

### 2.2 Trust Boundary and Integrity Assumptions (Normative)
The system’s integrity guarantees assume an **honest, NPAP-conformant adapter**.
*   **Trusted Computing Base (TCB):** The Namako Engine, hashing infrastructure, and schema validation logic form the TCB.
*   **Adapter Trust:** The adapter is treated as part of the user's project and is trusted to report faithfully unless otherwise stated.
*   **Integrity-Certified Definition:** The system proves *drift-free spec→resolution→declared execution* given a conformant adapter. It does not defend against malicious adapters (out of scope).

### 2.3 Explicit Identity vs. Derived Identity

Identities must survive file renames and refactors.
*   **Feature Identity:** Declared in-file (e.g., `@FID(connection)`), never derived from filenames.
*   **Row Identity:** Scenario Outlines must include an explicit `EID` column.

### 2.4 Baseline vs. Candidate Certification

CI never "generates" certification to check itself (tautology).
*   **Baseline:** The committed certification artifact (`certification.json`).
*   **Candidate:** Generated from the current working tree by the Engine.
*   **Verification:** `candidate === baseline` (STRICT equality).

---

## Part 3: The Six Goals

### Goal 1: Spec Unambiguity on Demand
*   **Mechanism:** `namako review`. Generates Challenge Packets requiring discriminating scenarios for ambiguous text.
*   **Strictness:** Operational ambiguity (multiple matches for actual step text) is a **hard error**.

### Goal 2: Scenario Completeness on Demand
*   **Mechanism:** Enforceable structural completeness.
*   **Strictness:** "Completeness" is defined as:
    *   Every Rule has at least one Scenario.
    *   Every Scenario step resolves uniquely under strict ambiguity policy.
    *   There are no missing steps (0 matches).
    *   The resolved plan is fully generated for all scenarios with zero errors.

> **Non-Goal (v9):** Namako does not attempt to automatically infer “intent coverage” beyond the Rule/Scenario structure. Deep semantic completeness is a review-driven process (`namako review`).

### Goal 3: Test Faithfulness on Demand
*   **Mechanism:** **Plan-Driven Execution.** The adapter has no autonomy to diverge from the spec's intent.

### Goal 4: Repeatable Perfection Loop
*   **Mechanism:** `namako verify` in CI ensures the certification tuple (spec + plan + registry) is locked and matches the committed baseline.

### Goal 5: Change Propagation as First-Class Capability
*   **Mechanism:** Precise hashing of Features (`feature_ast_hash`), Registries (`step_registry_hash`), and Plans (`resolved_plan_hash`).
*   **Result:** We know exactly which scenarios are invalidated by a binding change or semantic shift.

### Goal 6: Trustworthy, Audit-Grade Outputs
*   **Mechanism:** **Hash-based integrity evidence** with semantic separation.
*   **Artifacts:** `resolved_plan.json`, `run_report.json`, `certification.json`.

---

## Part 4: System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              SYSTEM ARCHITECTURE                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌────────────────────────────────────────────────────────────────────────────┐  │
│  │                         SPECIFICATION LAYER                                 │  │
│  │                                                                             │  │
│  │   specs/features/*.feature  (Normative Source)                              │  │
│  │   • Feature Identity (@FID)                                                 │  │
│  │   • Scenario Identity (@Snn)                                                │  │
│  │                                                                             │  │
│  └────────────────────────────────────────────────────────────────────────────┐  │
│                                       │                                          │
│                                       ▼                                          │
│  ┌────────────────────────────────────────────────────────────────────────────┐  │
│  │                         NAMAKO (Tooling Suite)                              │  │
│  │                                                                             │  │
│  │   ┌────────────────────┐          ┌────────────────────────────────────┐    │  │
│  │   │  NAMAKO CLI        │          │    NAMAKO ENGINE (Semantics)       │    │  │
│  │   │                    │          │                                    │    │  │
│  │   │ • lint / verify    │─────────▶│ • AST Normalization & Hashing      │    │  │
│  │   │ • update-cert      │─────────▶│ • Resolution -> RESOLVED PLAN      │    │  │
│  │   │                    │          │ • Strictness / Ambiguity Check     │    │  │
│  │   └────────────────────┘          │ • Certification Logic              │    │  │
│  │             │                     └────────────────────────────────────┘    │  │
│  │             │                                       │                       │  │
│  │             │   ┌───────────────────────────────────┘                       │  │
│  │             ▼   ▼                                                           │  │
│  │      (NPAP Protocol)                                                        │  │
│  │      1. adapter manifest -> Engine                                          │  │
│  │      2. Engine (Plan) -> adapter run                                        │  │
│  │             │                                                               │  │
│  │             ▼                                                               │  │
│  ┌────────────────────────────────────────────────────────────────────────────┐  │
│  │                          PROJECT ADAPTER LAYER                              │  │
│  │                                                                             │  │
│  │   ┌────────────────────────────────────────────────────────────────────┐   │  │
│  │   │  Naia / Any Project                                                │   │  │
│  │   │                                                                    │   │  │
│  │   │  • naia_namako (Adapter Binary)                                    │   │  │
│  │   │     ├─ export manifest (Semantic + Debug)                          │   │  │
│  │   │     └─ execute plan (BY ID ONLY) ──────────────────────────────────┘   │  │
│  │   │                                                                    │   │  │
│  │   └────────────────────────────────────────────────────────────────────┐   │  │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘

### 4.1 Shared Hashing Infrastructure (Normative)
Engine and adapters MUST use a shared reference implementation for canonical encoding, hash computation, and payload-hash construction.
*   **Typed Structures:** Hashed artifacts MUST be represented internally as typed structures with a fixed schema.
*   **Deterministic Ordering:** Any map-like structures MUST be represented using deterministically ordered maps (e.g., `BTreeMap`). Encoding MUST NOT depend on runtime map iteration order.
*   **Namako Hash Crate:** The `namako_hash` crate is normative for canonical encoding, hash computation, and fixture validation.
*   **Allowed Encoding:** The canonical encoding rules (e.g., specific CBOR profile) are defined by `namako_hash` and versioned by `hash_contract_version`. Implementations MUST validate against test vectors.
```

---

## Part 5: The Namako Engine & Resolution

### 5.1 The Resolved Execution Plan
The engine produces the **Resolved Execution Plan** (`resolved_plan.json`) before execution. This file must be fully self-contained and hashable.

**Header Fields (Required):**
- `hash_contract_version`
- `engine_version`
- `gherkin_parser_version`
- `cucumber_expressions_version`
- `resolution_config`: canonical object describing resolution settings (e.g., ambiguity policy, match mode). Included in hash.
- `feature_ast_hash` (The AST hash of the source feature)
- `step_registry_hash` (The hash of the Semantic Step Registry used for resolution)

**Per Scenario:**
1.  **Scenario Key:** `feature::Rule::Scenario` (e.g., `connection::R01::S01`).
2.  **Steps:** Ordered list of steps, each containing:
    - `effective_kind`: `Given`, `When`, or `Then` (Pre-calculated).
    - `step_text`: The exact AST text (**Required**, always present, and always included in `resolved_plan_hash`).
    - `binding_id`: The exact unique ID of the code to run.
    - `captures`: Array of **strings** extracted from the step text.
    - `docstring`: Normalized content (line endings normalized to `\n`), if present.
    - `datatable`: Normalized content, if present.
    -   `payload_hash`: Hash of the **execution payload** (canonical map of `effective_kind`, `binding_id`, `captures`, `docstring`, `datatable`, `step_text`) per Hash Contract.

### 5.2 Resolved Plan Hash
`resolved_plan_hash = blake3_256( canonical_encode( ResolvedPlanWithoutHashFields ) )`

*   **WithoutHashFields** means:
    *   omit `header.resolved_plan_hash`.
    *   omit any other `*_hash` fields that may appear in the plan.
*   The encoded object MUST include:
    *   all header fields (including versions + `resolution_config`).
    *   all scenario keys.
    *   step order.
    *   `effective_kind`, `step_text`, `binding_id`, `captures`, `docstring`, `datatable`.
*   The plan MUST be fully self-contained and deterministic under canonical encoding rules.

### 5.3 Resolution Logic & Validation
*   **Kind Inference:** `And`/`But` are resolved to their effective kind based on the AST.
*   **Expression Matching:** Uses `cucumber-expressions`.
*   **Signature Enforcement:**
    *   Engine validates that the extracted number of captures matches the binding's `captures_arity`.
    *   Engine validates that presence of DocString/DataTable matches the binding's signature.
    *   Mismatch -> **Hard Error**.

### 5.4 Strictness Policies
*   **Operational Ambiguity:** If a step matches >1 binding → **Hard Error**.
*   **Orphans:** If a binding is not used by any feature → **Hard Error**.
    *   *Mitigation:* Use `namako stub --binding <id>` to generate a placeholder scenario.
*   **Missing Steps:** If a step matches 0 bindings → **Hard Error**.

---

## Part 6: The ID Scheme & Stability

### Feature Identity
Features must define an ID using the `@FID` tag. Filenames are cosmetic.

```gherkin
@FID(connection)
Feature: Connection Lifecycle
```

### Rule & Scenario Identity
*   **Rule:** `@Rnn` (Unique per file).
*   **Scenario:** `@Snn` (Unique per Rule).
*   **ScenarioKey:** `feature_id::Rnn::Snn` (Globally unique).

### Scenario Outline Identity
Outlines must have an explicit `EID` column. Positional identity is banned.

```gherkin
  Examples:
    | EID | protocol |
    | E01 | udp      |
    | E02 | tcp      |
```
*   **Key:** `feature_id::Rnn::Snn::E<EID>`
*   **Hashing Semantics:** Example rows MUST be keyed by `EID`. In `FeatureAstNorm`, Examples are represented as an ordered structure derived from sorting by `EID` (lexicographic). Reordering rows in the `.feature` file MUST NOT change `feature_ast_hash`.

---

## Part 7: The Spec Surface & Canonicalization

### 7.1 AST Normal Form and Hashing (`feature_ast_hash`)
Text-based canonicalization is replaced by highly stable **AST-normal-form hashing**.
1.  Engine parses `.feature` → Gherkin AST.
2.  Engine converts AST → `FeatureAstNorm` (Canonical Internal Model).
3.  Engine serializes `FeatureAstNorm` deterministically (per Hash Contract).
4.  Engine hashes bytes to produce `feature_ast_hash`.

**`FeatureAstNorm` Schema:**
All strings MUST be Unicode-normalized to NFC prior to hashing.
-   **Feature:** `feature_id`, `feature_tags` (sorted), `rules[]` (ordered).
-   **Rule:** `rule_id`, `rule_tags` (sorted), `background_steps[]`, `scenarios[]` (ordered).
-   **Scenario:** `scenario_id`, `scenario_tags` (sorted), `steps[]`.
-   **Scenario Outline:** Same as Scenario, plus `examples` represented as `BTreeMap<EID, ExampleRowNorm>` (sorted by EID).
-   **Step:** `effective_kind`, `step_text` (exact AST string), `docstring` (normalized), `datatable` (exact cells).

### 7.2 Durations
Durations (`duration_ms`) are allowed in reports for UX/Profiling but **MUST NOT appear** in any hashed artifact (`feature_ast_hash`, `step_registry_hash`, `resolved_plan_hash`).

### 7.3 Gherkin Surface Contract (Normative)
Namako enforces a strict subset of Gherkin to ensure stability.

1.  **Allowed Constructs:** Exactly one Feature per file. Scenarios MUST appear under a `Rule`. **Scenarios directly under Feature are forbidden.**
2.  **Rule Requirements:** Each Rule must have exactly one unique `@Rnn` tag. Order is significant and hashed.
3.  **Background Policy:** `Background:` is allowed ONLY under a Rule. Steps are prepended to every Scenario in that Rule. Feature-level Background is forbidden.
4.  **Tag Inheritance:** `EffectiveTags` = Union(Feature, Rule, Scenario). Order irrelevant; hashed as sorted set.
5.  **Step Normalization:**
    -   `And`/`But` resolved to effective kind.
    -   DocStrings: line endings normalized to `\n`.
    -   DataTables: cell text preserved as provided by AST.

---

## Part 8: The Eight Invariants

### Invariant 1: Structural Tag Integrity
Every Feature, Rule, Scenario, and Example Row must have their respective `@FID`, `@Rnn`, `@Snn`, and `EID` identifiers.

### Invariant 2: Explicit Binding Identity
Every binding function exported by the Adapter MUST declare a stable `binding_id` and `impl_hash`.

### Invariant 3: Engine Supremacy
The Adapter MUST NOT perform text matching. It must execute exactly the `binding_id`s and payloads provided in the Resolved Plan.

### Invariant 4: No Orphan Bindings
Every binding in the registry MUST be used by at least one scenario.

### Invariant 5: Operational Determinism
Output ordering (lists, maps) must be deterministic. Certification must be deterministic regardless of map iteration order.

### Invariant 6: Single-Kind Binding Functions
A binding function matches exactly one kind (`Given`, `When`, `Then`).

### Invariant 7: Collision-Free Execution
Scenarios must run isolated. Adapters must use unique temp dirs and per-scenario `World` instances.

### Invariant 8: Explicit Certification Workflow
Certification is never updated implicitly. `verify` checks status; `update-cert` changes status.

---

## Part 9: The Namako Project Adapter Protocol (NPAP)

### 9.1 Versioning & Schema
All artifacts must include `npap_version = 2`. All tools MUST use strict JSON schema validation and reject unknown fields.

### 9.2 Command 1: `adapter manifest`
The adapter returns two registries. Only the **Semantic** registry is used for hashing and integrity.

**1. Semantic Step Registry (Hashed):**
-   **Bindings:**
    -   `id`: Unique binding ID.
    -   `kind`: Given/When/Then.
    -   `expression`: Cucumber expression string.
    -   `signature`:
        -   `captures_arity`: u32
        -   `accepts_docstring`: bool
        -   `accepts_datatable`: bool
    -   `impl_hash`: **Drift Signal.** `blake3_256( canonical_encode( ImplHashInputBundle ) )`.
        -   `ImplHashInputBundle` MUST include:
            1.  `expanded_binding_source` (macro-expanded or fully elaborated implementation).
            2.  `toolchain_fingerprint` (e.g., `rustc -Vv`).
            3.  `dependency_fingerprint` (e.g., hash of `Cargo.lock`).
        -   **Policy:** `impl_hash` MUST change when the effective implementation changes. Toolchain or dependency changes MAY invalidate certification (this is acceptable).
-   **Parameter Types:** `{ name, regex }`.

**2. Debug Step Registry (Unhashed):**
-   `source_loc`, documentation, human notes.

### 9.3 Command 2: `adapter run`
*   **Purpose:** Execute the **Resolved Plan**.
*   **Input:** `--plan <path/to/resolved_plan.json>`
*   **Output:** `--out <path/to/run_report.json>`
*   **Runtime Rules:**
    *   Adapter MUST execute steps **by Binding ID only** using a direct lookup.
    *   Adapter MUST NOT consult the semantic registry to resolve steps during `run` (registry is only used for freshness + binding existence/signature checks).
    *   Adapter MUST treat `step_text` as non-executable metadata.
    *   Adapter MUST compute `executed_payload_hash` from the actual runtime values passed to the binding.
*   **Freshness Check:** The Adapter **MUST refuse** to execute a plan unless:
    -   `step_registry_hash` in plan matches the current semantic manifest hash.
    -   All `binding_id`s in the plan exist.
    -   Signatures (arity/payloads) match the implementation.

### 9.4 Artifact Schemas

**Resolved Plan (`resolved_plan.json` - Engine Output)**
```json
{
  "header": {
    "npap_version": 2,
    "hash_contract_version": "namako-canon-v1+blake3-256",
    "engine_version": "...",
    "resolution_config": { "ambiguity": "strict", ... },
    "feature_ast_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..."
  },
  "scenarios": {
    "connection::R01::S01": {
      "steps": [
        {
          "effective_kind": "Given",
          "step_text": "server is running", 
          "binding_id": "conn-001",
          "captures": [],
          "docstring": null,
          "datatable": null,
          "payload_hash": "..."
        }
      ]
    }
  }
}
```

**Run Report (`run_report.json` - Adapter Output)**
*   **Canonical Ordering:** Scenarios ordered by `scenario_key`. Steps in plan order. Keys sorted.
*   **Header Echo:** Must echo `feature_ast_hash`, `step_registry_hash`, `resolved_plan_hash`, `hash_contract_version`, `npap_version` from the plan.
```json
{
  "header": { 
    "npap_version": 2, 
    "hash_contract_version": "namako-canon-v1+blake3-256",
    "feature_ast_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..." 
  },
  "scenarios": [
    {
      "scenario_key": "connection::R01::S01",
      "status": "Passed",
      "steps": [
        {
          "planned_binding_id": "conn-001",
          "executed_binding_id": "conn-001",
          "planned_payload_hash": "...",
          "executed_payload_hash": "...",
          "status": "Passed"
        }
      ]
    }
  ]
}
```

---

## Part 10: Runtime & Execution: The Plan-Driven Model

1.  **Namako Lint:** Engine parses features + manifest. Resolves plan. Validates signatures.
2.  **Namako Run:**
    *   Engine ensures plan is fresh.
    *   Writes `resolved_plan.json`.
    *   Calls `adapter run --plan ...`.
    *   Adapter performs **Header Check** (refuses stale/mismatched plans).
    *   Adapter loads plan, spawns test threads.
    *   Adapter writes `run_report.json`.
3.  **Verification:** Engine reads `run_report.json`.
    *   Verifies `planned_binding_id` == `executed_binding_id` for every step.
    *   Verifies `planned_payload_hash` == `executed_payload_hash` for every step.
    *   Verifies report header's `feature_ast_hash`, `step_registry_hash`, `resolved_plan_hash`, `hash_contract_version`, `npap_version` match the plan header exactly.
    *   Computes `bindings_used_hash`.
    *   Generates Candidate Tuple.

---

## Part 11: Certification & Integrity Workflow

### 11.1 The Tuple
The **Integrity Identity Tuple** (`certification.json`) is the heart of the system.
*   `hash_contract_version`
*   `engine_version` (Namako)
*   `gherkin_parser_version`
*   `cucumber_expressions_version`
*   `feature_ast_hash` (From AST-normal-form)
*   `step_registry_hash` (Semantic only)
*   `resolved_plan_hash` (The proof of resolution stability)
*   `bindings_used_hash` (Optional but recommended for diffs)
*   **Optional Recommended Fields** (if present, computed via canonical encoding + blake3-256):
    *   `adapter_build_hash`
    *   `cargo_lock_hash`
    *   `rustc_version`

### 11.2 Verification Logic
`namako verify` performs a deep comparison:
1.  **Candidate Generation:** Computes `resolved_plan_hash` from the candidate `resolved_plan.json` produced by the Engine.
2.  **Comparison:** `Candidate.Tuple === Baseline.Tuple`.
3.  **Result:** Any mismatch in ANY field is a hard failure.

Verification validates the run report against the plan. This constitutes integrity evidence under the assumption that the adapter truthfully emits runtime events per NPAP.

### 11.3 Canonical Encoding & Hashing Contract (Normative)
1.  **Typed Canonicalization:** Hashed artifacts MUST be encoded from typed structures that match strict schemas.
2.  **Deterministic Ordering:**
    *   All maps MUST be deterministically ordered (e.g., sorted keys via ordered map type or lexicographic byte sort).
    *   All lists MUST have stable, specified ordering.
3.  **No Floats:** Floats are forbidden in hashed artifacts.
4.  **No Unknown Fields:** Any consumed artifact MUST be schema-validated and reject unknown fields.
5.  **Presence Rules:** For hashed artifacts, optional fields MUST be **omitted when absent** (or consistently null). The rule MUST be uniform.
6.  **Canonical Bytes:** The canonical byte encoding MUST be produced by the `namako_hash` implementation and MUST be stable for the `hash_contract_version`.
7.  **Hash Algorithm:** `hash_fn = BLAKE3-256` (lowerhex).
8.  **Versioning:** Any change to canonical encoding rules requires bumping `hash_contract_version` and regenerating fixtures.

#### 11.3.1 Hash Field Exclusion Rule (Normative)
*   All `*_hash` fields are **excluded** from the bytes used to compute any hash over the containing object.
*   Concretely: when computing `resolved_plan_hash`, the plan header is encoded **with `resolved_plan_hash` omitted**.
*   Same for any object containing its own hash value (e.g., `feature_ast_hash`, `step_registry_hash`, or `bindings_used_hash`).
*   Hashes are computed on **canonical bytes** (per 11.3), never on JSON text.

#### 11.3.2 Conformance Fixtures (Normative)
*   The repo MUST include a fixture suite containing:
    *   canonical input (structured/JSON).
    *   canonical encoded bytes (optional).
    *   expected hash outputs.
*   **Scope:** Fixtures MUST cover at least: `FeatureAstNorm`, `SemanticStepRegistry`, and `ResolvedPlan`.
*   **Validation:** CI MUST run fixtures on supported platforms and fail if any mismatch occurs.

### 11.5 Bindings Used Hash
`bindings_used_hash` is computed from the canonical ordered list of unique binding IDs referenced in `resolved_plan.json` (lexicographically sorted).

---

## Part 12: The Namako CLI

### Core Workflow
*   `namako manifest`: Debug. Shows adapter steps.
*   `namako lint`: Resolves everything. Checks for strictness/ambiguity errors.
*   `namako run`: Lint + Execute Plan + Report.
*   `namako stub --binding <id>`: Generates a minimal scenario for an orphan in a Rule-compliant way.

### Integrity Management
*   `namako verify`: **CI Gate.** Fails if current state != baseline.
*   `namako update-cert`: **Manual Action.** Overwrites baseline with current candidate. **MUST refuse** to write baseline unless:
    *   `namako lint` passes with zero errors.
    *   `namako run` completes and all scenarios are `Passed`.
*   `namako status`: detailed diff of what has drifted.

### QA Tools
*   `namako review`: Generate semantic challenge packets.

---

## Part 13: Naia Integration

### 13.1 `namako.toml`
```toml
features_root = "specs/features"
adapter_cmd = ["cargo", "run", "-q", "-p", "naia_namako", "--"]
artifacts_dir = "target/namako_artifacts"
baseline_cert = "specs/certification.json"
```

### 13.2 The `naia_namako` Adapter
Must be updated to accept `--plan`.
*   **No Parsing:** The adapter should not parse Gherkin.
*   **No Regex:** The adapter should not execute regex matching at runtime.
*   **Lookup Map:** `HashMap<BindingId, Box<dyn Fn(World, Args)>>`.
*   **Non-Autonomous:** Adapter MUST execute by Binding ID only.

---

## Part 14: Migration Plan

### Phase 1: Engine v9
*   Implement `FeatureAstNorm` and `feature_ast_hash`.
*   Implement `resolved_plan_hash`.
*   Implement Semantic vs Debug registry separation.

### Phase 2: Adapter Refactor
*   Refactor `naia_namako` to emit Semantic+Debug manifests.
*   Implement `impl_hash` generation (proc-macro/build-script) that constructs the `ImplHashInputBundle` (with fingerprints).
*   Enforce Rule-only scenarios and `@FID/@Rnn/@Snn/EID`.
*   Implement stale plan rejection.

### Phase 3: Lock-In
*   Run `namako update-cert` to generate initial v9 spec.
*   Enable `namako verify` in CI.

---

## Part 15: Definition of Done

The system is live when:
1.  **Certification is Precise:** `certification.json` includes `resolved_plan_hash` and `feature_ast_hash` (derived solely from AST-normal-form).
2.  **Registry is Split:** Semantic registry is hashed; debug registry is not. Each binding includes `signature` and `impl_hash`.
3.  **Signatures are Enforced:** Adapter enforces arity and payload expectations at runtime.
4.  **Stale Plans are Rejected:** Adapter checks header hashes before running.
5.  **Specs conform to Contract:** All features use Rule-only structure and explicit IDs.
6.  **CI is Strict:** `namako verify` passes in CI.

## Appendix A: The AI-Assisted Spec-Driven Development Loop

This section defines the **mandatory, repeatable workflow** for AI-assisted spec-driven development using Namako. It is designed to be executed in a **tight feedback loop** by a coding agent and reviewed/approved by a developer.

### Core Principle
**Namako is the authority.** The agent does not “guess correctness.” It repeatedly:
**run → classify → minimal edit → rerun** until all gates are satisfied.

### Non-Negotiable Rules
1. **Never update certification implicitly.**
   - The agent MUST NOT run `namako update-cert` without explicit developer approval.
2. **Always lint before run.**
   - The agent MUST run `namako lint` and resolve all lint failures before `namako run`.
3. **One failure bucket at a time.**
   - The agent MUST classify failures (see “Failure Buckets”) and fix the smallest change that eliminates that bucket before continuing.

---

## The Tight Loop (Slice-Based Workflow)

Work in **small slices** (typically one `Rule` or a small set of scenarios). Do not expand scope until the current slice is certified.

### Step 1: Requirements Capture (Developer ↔ Agent)
**Goal:** convert an idea into a crisp, testable behavioral contract.

**Agent actions:**
- Ask clarifying questions until requirements are unambiguous.
- Draft a temporary Markdown requirements doc capturing:
  - the behavior intent
  - key invariants
  - edge cases
  - observable outcomes

**Exit condition:**
- Developer confirms the behavior description is correct and complete enough to formalize.

### Step 2: Convert to Normative Spec (.feature)
**Goal:** the `.feature` file becomes the single normative spec surface for that behavior.

**Agent actions:**
- Convert the Markdown requirements into a `.feature` file:
  - Put rationale/prose into **Gherkin comments** (`# ...`) near the relevant Rule/Scenario.
  - Enforce identity + structure rules:
    - `@FID(...)` on Feature
    - `@Rnn` on each Rule
    - `@Snn` on each Scenario
    - Scenario Outlines require explicit `EID` column
    - Scenarios MUST be under a Rule (no Feature-level scenarios)
- Delete or archive the temporary Markdown (policy choice); the `.feature` is the normative source.

**Exit condition:**
- The `.feature` reads coherently to a human and appears implementable.

### Step 3: Scenario Integrity Loop (Spec ↔ Namako Engine)
**Goal:** ensure the `.feature` is structurally valid, unambiguous, and scenario-complete.

**Agent loop:**
1. Run: `namako lint`
2. If `lint` fails:
   - Fix structure/IDs if violated.
   - If ambiguity or missing discriminators: run `namako review` and implement the required challenge scenarios.
   - Iterate until `lint` is clean.

**Exit condition:**
- `namako lint` passes with:
  - no missing steps
  - no operational ambiguity
  - all structure/ID invariants satisfied

### Step 4: Binding/Test Faithfulness Loop (Plan ↔ Adapter)
**Goal:** ensure scenarios are faithfully represented as bound BDD tests with plan-driven integrity.

**Agent loop:**
1. Run: `namako manifest` (inspect the semantic registry)
2. Run: `namako lint` (resolution + signature enforcement)
3. Run: `namako run` (plan execution + run report)
4. Run: `namako verify` (candidate tuple strictly equals baseline)

**Agent obligations when failures occur:**
- If resolution/signature fails: fix binding signatures/expressions/parameter types (never “loosen” spec to hide ambiguity).
- If execution faithfulness fails (e.g., binding ID mismatch or payload hash mismatch):
  - treat as an adapter/binding bug and fix the execution path so it obeys the resolved plan exactly.
- If `verify` fails:
  - produce a diff (`namako status`) and an explanation of what drifted and why.
  - do NOT run `update-cert` without developer approval.

**Exit condition:**
- `namako lint` PASS
- `namako run` PASS (all scenarios passed)
- `namako verify` PASS

### Step 5: Implement the System (Tests → Working Code)
**Goal:** implement/modify the system under test until the bound scenarios all pass.

**Agent loop:**
- Make minimal implementation changes.
- Re-run `namako lint` → `namako run` until green.

**Exit condition:**
- All scenarios pass via `namako run` with no lints.
