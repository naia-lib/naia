# NAMAKO_PLAN_FINAL.md

## The Authoritative Specification System for Naia — KISS MVP (v1) + Armor Plating Roadmap (v2+)

---

## Table of Contents

1. [Executive Summary](#part-1-executive-summary)
2. [Thesis: v1 is KISS MVP, v2+ is Armor Plating](#part-2-thesis-v1-is-kiss-mvp-v2-is-armor-plating)
3. [Canonical Repo & Crate Architecture](#part-3-canonical-repo--crate-architecture)
4. [Step Macro UX and Binding Identity](#part-4-step-macro-ux-and-binding-identity)
5. [Namako v1: The Future-Proof KISS MVP](#part-5-namako-v1-the-future-proof-kiss-mvp)
6. [NPAP v1: Adapter Protocol](#part-6-npap-v1-adapter-protocol)
7. [Hashing & Identity: v1 Contract](#part-7-hashing--identity-v1-contract)
8. [Where v1 is Intentionally NOT Publish-Grade](#part-8-where-v1-is-intentionally-not-publish-grade)
9. [The AI-Assisted Spec-Driven Development Loop (v1)](#part-9-the-ai-assisted-spec-driven-development-loop-v1)
10. [Namako v2+ — Armor Plating (Deferred Publish-Grade Features)](#part-10-namako-v2--armor-plating-deferred-publish-grade-features)
    - [10.11 Multi-Language Support](#1011-multi-language-support-language-neutral-engine-language-specific-adapters)
    - [10.12 Adapter SDKs](#1012-adapter-sdks-v2)
    - [10.13 Cross-Language Hashing & Conformance](#1013-cross-language-hashing--conformance-v2)
    - [10.14 Adapter Certification Tooling](#1014-adapter-certification-tooling-v2)
11. [Definition of Done (v1)](#part-11-definition-of-done-v1)
12. [Appendix: No-drop Checklist (v9 Concept Trace)](#appendix-no-drop-checklist-v9-concept-trace)

---

## Part 1: Executive Summary

This document specifies **Namako**, the authoritative spec-driven development system for Naia. Namako is a fork of the `cucumber` crate, renamed and pruned for our purposes.

The core workflow is:

```
spec (.feature) → engine(resolve) → plan → adapter(execute) → evidence → verify
```

**Namako v1** is the **minimum future-proof KISS MVP** that Connor can use immediately for Naia development. It provides:
- Strict resolution of `.feature` files to step bindings
- Plan-driven execution (adapter executes by binding ID only, no text matching)
- Hash-based integrity evidence
- CI-gated certification via `namako verify`

**Namako v2+** captures all hardening features deferred from v1 but designed to be forward-compatible.

### The Core Insight: Plan-Driven Integrity

Drift occurs when the linter thinks a step matches Binding A, but the runtime matches Binding B. **Namako eliminates this class of error entirely.**

The **Namako Engine** is the sole source of matching logic. It resolves every step into a **Resolved Execution Plan**. The project adapter is **structurally forbidden** from performing text matching; it simply executes the Binding IDs dictated by the Engine.

### Language-Neutral by Design

Namako is **language-agnostic**: the engine/CLI is a Rust tool, but adapters MAY be implemented in **any programming language** (Rust, JS/TS, Python, Go, C++, etc.). The adapter protocol (NPAP) is the only cross-language integration boundary. v1 ships with Rust adapter support for Naia; v2+ expands to official SDKs and conformance tooling for other ecosystems.

---

## Part 2: Thesis: v1 is KISS MVP, v2+ is Armor Plating

### 2.1 Separation of Concerns

This document explicitly separates:

| Concern | v1 (KISS MVP) | v2+ (Armor Plating) |
|---------|---------------|---------------------|
| Scope | Minimum viable for Naia self-development | Publish-grade hardening |
| Timeline | Build now | Build later (captured here) |
| Identity stability | Expression-based binding IDs | Refactor-stable explicit IDs |
| Feature hashing | Feature fingerprint (simpler) | Full FeatureAstNorm |
| Orphan policy | Warning only | Hard error + mitigation tools |
| Encoding | Canonical JSON | CBOR profiles, conformance fixtures |
| Language support | Rust adapter (Naia) | Multi-language SDKs + conformance |

### 2.2 Design Principle: No Dead Ends

v1 is designed such that every v2+ feature can be adopted incrementally via:
- Version bumps (`hash_contract_version`, `binding_id_scheme`)
- Additive schema changes
- Identity regeneration (via `update-cert`)

No v1 decision MUST be reversed to adopt v2+.

---

## Part 3: Canonical Repo & Crate Architecture

### 3.1 Namako Repo (fork of `cucumber`, renamed/pruned)

The Namako repo MUST contain exactly these crates:

#### 3.1.1 `namako` (lib)
The core engine/runtime library. Contains:
- Resolution logic
- Artifact schemas (or re-exports)
- Hashing utilities (or re-exports)
- Verification logic

Dependencies:
- `gherkin` (Gherkin parser)
- `cucumber_expressions` (expression matching)
- `namako_codegen` (proc-macros)

#### 3.1.2 `namako_codegen` (proc-macro)
Formerly `cucumber_codegen`. Owns:
- Step macros (`#[given(...)]`, `#[when(...)]`, `#[then(...)]`)
- Registry generation

#### 3.1.3 `namako_cli` (bin)
Provides the CLI commands:
- `manifest` — Debug: prints adapter semantic registry + hashes
- `lint` — Resolve features, generate resolved_plan, fail on strict errors
- `run` — Lint + execute plan via adapter + produce run report + validate integrity
- `verify` — CI gate: candidate identity == baseline identity
- `update-cert` — Manual: writes baseline cert (MUST refuse unless prerequisites satisfied)
- `status` — Optional in v1: diff identity vs metadata

### 3.2 Naia Repo (project integrating Namako)

The Naia repo MUST contain exactly these crates:

#### 3.2.1 `naia_test_harness` (lib)
This is a rename of existing `naia_test` (the test harness).
- Implements the Namako "World" type used by step bindings
- Encapsulates `naia_test_harness::Scenario` (1 server + N clients using local transport)
- Concurrency-immune by design (local channels, not sockets)

#### 3.2.2 `naia_tests` (lib)
Contains all step binding functions.
- Step functions use Namako step macros from `namako_codegen`
- Depends on `naia_test_harness` to construct World and drive scenarios

#### 3.2.3 `naia_namako` (bin)
The NPAP adapter binary for Naia.
- Links in `naia_tests` so all bindings/registry/dispatch are present
- Implements:
  - `naia_namako manifest` — prints registry JSON
  - `naia_namako run --plan ... --out ...` — executes resolved plan by binding_id only, emits run_report

### 3.3 File Locations (Normative)

| Artifact | Location |
|----------|----------|
| `.feature` files | `specs/features/**/*.feature` (Naia repo) |
| Baseline certification | `specs/certification.json` (Naia repo) |
| Artifacts directory | `target/namako_artifacts/` (Naia repo, or as configured) |

---

## Part 4: Step Macro UX and Binding Identity

### 4.1 UX Requirement: One Macro + One String (Hard Requirement)

Step functions MUST be declared using exactly:

```rust
#[given("...")]
fn some_given_step(world: &mut World) { ... }

#[when("...")]
fn some_when_step(world: &mut World) { ... }

#[then("...")]
fn some_then_step(world: &mut World) { ... }
```

Each macro takes **exactly one string argument**.
- No additional attributes
- No additional metadata
- No embedded IDs in strings
- No optional parameters

### 4.2 Generated Binding ID (Normative)

User code MUST NOT contain explicit binding IDs. The system MUST ALWAYS generate `binding_id` deterministically from:
- `effective_kind` (Given/When/Then)
- `expression_string` (the literal string inside the macro)

#### 4.2.1 Binding ID Scheme (v1, Normative)

Define `expr_norm` as the macro string normalized by:
1. Unicode normalization to NFC
2. Newline normalization to `\n`

> **Note:** v1 MUST NOT add other normalizations (e.g., whitespace collapsing). Keep it simple.

Define:
```
binding_id = blake3_256_lowerhex( "namako-binding-id-v1|" + kind + "|" + expr_norm )
```

The semantic registry MUST include:
```
binding_id_scheme = "kind+expr_norm|namako-binding-id-v1|blake3-256-lowerhex"
```

`binding_id_scheme` MUST be included in the `step_registry_hash` computation.

> **v2+ Note:** The binding-id scheme is chosen specifically because it is **portable across languages and tooling**. Any adapter in any language can compute the same `binding_id` from the same `(kind, expression_string)` pair using the documented algorithm and BLAKE3.

#### 4.2.2 Collision Rule (Normative)

If two bindings in a single project produce the same `(kind, expr_norm)`:
- That is a **hard error** (registry construction MUST fail).
- Rationale: identity collision creates operational ambiguity.

### 4.3 Dispatch Rule (Normative)

The adapter MUST:
- Execute steps **only by binding_id** using a direct lookup/dispatch table
- NOT perform text matching or regex at runtime
- Treat `step_text` as metadata only

---

## Part 5: Namako v1: The Future-Proof KISS MVP

### 5.1 v1 Scope: What is IN

v1 MUST include:

| Capability | Description |
|------------|-------------|
| Gherkin parsing | Parse `.feature` files via `gherkin` crate |
| Step resolution | Resolve steps to bindings via `cucumber_expressions` |
| Strict resolution errors | Missing steps (0 matches) → hard error |
| | Ambiguity (>1 match) → hard error |
| | Signature mismatch → hard error |
| Resolved plan artifact | `resolved_plan.json` |
| Run report artifact | `run_report.json` |
| Certification artifact | `certification.json` (baseline + candidate concept) |
| Deterministic identity tuple | See §7 |
| CI gate | `namako verify` (strict identity compare) |
| Manual baseline update | `namako update-cert` (only when explicitly invoked + prerequisites satisfied) |

### 5.2 v1 Scope: What is OUT (Deferred to v2+)

v1 MUST NOT require:

| Deferred Feature | Rationale |
|------------------|-----------|
| Full FeatureAstNorm hashing | Simpler fingerprint is sufficient for v1 |
| Explicit ID scheme (`@FID/@Rnn/@Snn/EID`) | Expression-based IDs are acceptable for v1 |
| Orphan binding hard errors | v1 MAY warn; v2+ makes it a hard error |
| Challenge packets / `namako review` | Deferred to v2+ |
| CBOR canonical encoding profiles | v1 uses canonical JSON; v2+ may migrate |
| Malicious adapter defense | Out of scope (trusted adapter assumption; v2+ adds conformance tooling) |
| Conformance fixtures with canonical bytes | Deferred to v2+ |
| `resolution_semantics_id` | Deferred to v2+; v1 uses simpler versioning |

### 5.3 v1 CLI Commands (Normative)

#### `namako manifest`
**Purpose:** Debug. Prints adapter semantic registry + hashes.

#### `namako lint`
**Purpose:** Resolve features + generate resolved_plan + fail on strict errors.

**Behavior:**
1. Parse all `.feature` files
2. Fetch adapter manifest (semantic registry)
3. Resolve each step to exactly one binding
4. Validate signatures (captures arity, docstring/datatable expectations)
5. Generate `resolved_plan.json`
6. Exit 0 on success, non-zero on any error

**Strict Errors:**
- Missing step (0 matches)
- Ambiguous step (>1 match)
- Signature mismatch

#### `namako run`
**Purpose:** Lint + execute plan via adapter + produce run report + validate integrity.

**Behavior:**
1. Execute `lint` (fail if lint fails)
2. Invoke adapter: `adapter run --plan <resolved_plan.json> --out <run_report.json>`
3. Validate run report integrity (see §7.4)
4. Exit 0 on success, non-zero on any failure

#### `namako verify`
**Purpose:** CI gate. Candidate identity MUST equal baseline identity.

**Behavior:**
1. Ensure a `run_report.json` exists and corresponds to current `resolved_plan.json`
2. Validate run report integrity (see §7.4)
3. Compare candidate identity to baseline identity with strict equality
4. Exit 0 if match, non-zero if any mismatch

**Prerequisite:** A successful `namako run` MUST have completed.

#### `namako update-cert`
**Purpose:** Manual action. Overwrites baseline certification with current candidate.

**Behavior:**
1. MUST refuse to write baseline unless:
   - `namako lint` passes with zero errors
   - `namako run` completes and all scenarios are `Passed`
2. If prerequisites satisfied, write `certification.json`

**Rationale:** Certification is never updated implicitly.

#### `namako status` (Optional in v1)
**Purpose:** If present, clearly diff identity vs metadata.

**Behavior:**
- Show identity fields that differ (blocking)
- Show metadata fields that differ (informational)

---

## Part 6: NPAP v1: Adapter Protocol

### 6.0 Language Neutrality (Normative)

NPAP is **language-neutral**. Adapters MAY be implemented in any programming language as long as they:
- Implement the `manifest` and `run` commands per this specification
- Obey all schema and invariant requirements
- Dispatch by `binding_id` only (no runtime text matching)

The Namako Engine/CLI MUST treat the adapter as an **external executable** invoked via the configured `adapter_cmd`. The engine MUST NOT depend on project language runtimes.

### 6.1 Versioning

All artifacts MUST include:
- `npap_version` — Protocol version (v1: use `1`)
- `hash_contract_version` — Identifies encoding + hashing rules (v1: `"namako-v1-json+blake3-256"`)

### 6.2 Command: `adapter manifest`

The adapter MUST implement:
```
naia_namako manifest
```

Returns the **semantic step registry** as JSON.

#### 6.2.1 Semantic Step Registry (Normative)

**Per Binding:**

| Field | Type | Description |
|-------|------|-------------|
| `binding_id` | string | Generated per §4.2 |
| `kind` | string | `"Given"`, `"When"`, or `"Then"` |
| `expression` | string | The cucumber expression string |
| `signature.captures_arity` | u32 | Number of captures expected |
| `signature.accepts_docstring` | bool | Whether binding accepts docstring |
| `signature.accepts_datatable` | bool | Whether binding accepts datatable |
| `impl_hash` | string | Drift signal (see §6.2.2) |

**Registry Header:**

| Field | Type | Description |
|-------|------|-------------|
| `npap_version` | u32 | Protocol version |
| `hash_contract_version` | string | Encoding + hashing rules |
| `binding_id_scheme` | string | Per §4.2.1 |
| `step_registry_hash` | string | Hash of the semantic registry |

#### 6.2.2 `impl_hash` (v1 Requirements)

`impl_hash` MUST change when the binding implementation changes.

**v1 Approach (Pragmatic):**
- Compute from a normalized source fingerprint of the binding function
- Normalization:
  - UTF-8 + Unicode NFC
  - Newlines normalized to `\n`
  - MAY exclude comments (implementation choice)
  - MAY exclude absolute paths (implementation choice)

`impl_hash = blake3_256_lowerhex( normalized_source_fingerprint )`

> **Note:** v2+ may strengthen this (see §10).

### 6.3 Command: `adapter run`

The adapter MUST implement:
```
naia_namako run --plan <resolved_plan.json> --out <run_report.json>
```

#### 6.3.1 Runtime Rules (Normative)

The adapter:
1. MUST refuse to run if plan's `step_registry_hash` does not match current manifest hash
2. MUST refuse to run if any `binding_id` in plan does not exist in registry
3. MUST execute steps **by binding_id dispatch only** (no text matching)
4. MUST treat `step_text` as non-executable metadata
5. MUST compute `executed_payload_hash` using the same rules as `planned_payload_hash`
6. MUST emit `executed_impl_hash` (from semantic registry entry of invoked binding)

#### 6.3.2 Freshness Check (Normative)

Before execution, the adapter MUST verify:
- `plan.header.step_registry_hash == current_manifest.step_registry_hash`
- All `binding_id`s in plan exist in registry
- Signatures match (arity, docstring, datatable expectations)

If any check fails, the adapter MUST refuse to execute and exit non-zero.

### 6.4 Artifact Schemas

#### 6.4.1 Resolved Plan (`resolved_plan.json`)

```json
{
  "header": {
    "npap_version": 1,
    "hash_contract_version": "namako-v1-json+blake3-256",
    "feature_fingerprint_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..."
  },
  "scenarios": {
    "<scenario_key>": {
      "steps": [
        {
          "effective_kind": "Given",
          "step_text": "server is running",
          "binding_id": "abc123...",
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

**Scenario Key:** For v1, use a deterministic key derived from file path + scenario name. v2+ may adopt explicit IDs.

#### 6.4.2 Run Report (`run_report.json`)

```json
{
  "header": {
    "npap_version": 1,
    "hash_contract_version": "namako-v1-json+blake3-256",
    "feature_fingerprint_hash": "...",
    "step_registry_hash": "...",
    "resolved_plan_hash": "..."
  },
  "scenarios": [
    {
      "scenario_key": "<scenario_key>",
      "status": "Passed",
      "steps": [
        {
          "planned_binding_id": "abc123...",
          "executed_binding_id": "abc123...",
          "planned_payload_hash": "...",
          "executed_payload_hash": "...",
          "executed_impl_hash": "...",
          "status": "Passed"
        }
      ]
    }
  ]
}
```

**Ordering:**
- Scenarios: ordered by `scenario_key` (lexicographic)
- Steps: in plan order
- Object keys: sorted (for determinism)

**Header Echo:** The run report MUST echo the plan header fields exactly.

### 6.5 Execution Payload Contract (Normative)

The **Execution Payload** for each step consists of:
- `effective_kind`
- `binding_id`
- `captures` (array of strings)
- `docstring` (normalized string or null)
- `datatable` (normalized cells or null)
- `step_text` (exact AST string)

**Normalization Rules:**
- DocStrings: line endings normalized to `\n`
- DataTables: exact cell strings from AST
- Strings: Unicode normalized to NFC
- Optional fields: omitted when absent (consistent rule)

**Payload Hash:**
```
payload_hash = blake3_256_lowerhex( canonical_json_encode( ExecutionPayload ) )
```

---

## Part 7: Hashing & Identity: v1 Contract

### 7.1 Hash Contract Versioning (Normative)

v1 MUST define:
```
hash_contract_version = "namako-v1-json+blake3-256"
```

This identifies:
- Canonical JSON encoding (sorted keys, deterministic omission, UTF-8)
- BLAKE3-256 hash algorithm (lowerhex output)

This version string MUST be included in every hashed artifact header.

### 7.2 Self-Hash Exclusion Rule (Normative)

When hashing an object:
- Omit **only** the field that stores that object's own hash
- Do NOT omit other hash fields that are part of the object's identity

**Example:**
- When computing `resolved_plan_hash`, omit `header.resolved_plan_hash`
- Do NOT omit `header.step_registry_hash` (it's input, not output)

### 7.3 v1 Identity Tuple (Normative)

The certification artifact (`certification.json`) contains `{ identity, metadata }`.

**Identity (strictly compared by `verify`):**

| Field | Description |
|-------|-------------|
| `hash_contract_version` | Encoding + hashing rules |
| `feature_fingerprint_hash` | Hash of feature content (v1: simpler than FeatureAstNorm) |
| `step_registry_hash` | Hash of semantic step registry |
| `resolved_plan_hash` | Hash of resolved execution plan |

**Metadata (recorded, not compared for pass/fail):**

| Field | Description |
|-------|-------------|
| `engine_version` | Namako version |
| `adapter_build_info` | Optional: adapter version/build |
| `cargo_lock_hash` | Optional: for reproducibility |
| `rustc_version` | Optional: for reproducibility |

#### 7.3.1 Feature Fingerprint Hash (v1)

For v1, compute a simpler feature fingerprint:
```
feature_fingerprint_hash = blake3_256_lowerhex(
  canonical_json_encode( FeatureFingerprint )
)
```

Where `FeatureFingerprint` includes:
- All feature file paths (sorted)
- For each file: hash of UTF-8 content after:
  - Unicode normalization to NFC
  - Newline normalization to `\n`

> **Note:** v2+ adopts full FeatureAstNorm for stability under cosmetic edits.

### 7.4 v1 Verification Rules (Normative)

`namako verify` MUST:

1. **Ensure run report exists** and corresponds to current resolved plan
2. **Validate run report integrity:**
   - For every step: `planned_binding_id == executed_binding_id`
   - For every step: `planned_payload_hash == executed_payload_hash`
   - For every step: `executed_impl_hash` matches registry `impl_hash` for the binding
   - Report header echoes match plan header (`feature_fingerprint_hash`, `step_registry_hash`, `resolved_plan_hash`, `hash_contract_version`, `npap_version`)
3. **Compare candidate identity to baseline identity:**
   - Strict field-by-field equality on identity fields
   - Any mismatch → hard failure

### 7.5 Canonical JSON Encoding (v1, Normative)

For v1, use canonical JSON:
- Object keys: sorted lexicographically
- No trailing commas
- No comments
- UTF-8 encoding
- Omit optional fields when absent (do not use `null`)
- Numbers: no leading zeros, no trailing decimal zeros
- Strings: minimal escaping (only required escapes)

---

## Part 8: Where v1 is Intentionally NOT Publish-Grade

v1 is explicitly a **KISS MVP for self-development**. The following limitations are accepted:

### 8.1 Expression-Based Binding IDs

The generated `binding_id` ties identity to expression strings.
- Editing an expression string changes its `binding_id`
- This is treated as identity drift requiring `update-cert`
- This is acceptable for v1 (self-development)
- v2+ may adopt stable explicit IDs for publish-grade stability

### 8.2 Simpler Feature Hashing

v1 uses feature fingerprint (content hash) rather than FeatureAstNorm.
- Cosmetic edits (whitespace, comments) may change hash
- This is acceptable for v1 (self-development)
- v2+ adopts full FeatureAstNorm for cosmetic-change immunity

### 8.3 No Explicit Structural IDs

v1 does not require `@FID`, `@Rnn`, `@Snn`, `EID` tags.
- Scenario identity derived from file path + name
- This is acceptable for v1 (self-development)
- v2+ may enforce explicit IDs for refactor stability

### 8.4 Orphan Bindings Are Warnings

v1 may warn on orphan bindings (bindings not used by any scenario).
- Not a hard error in v1
- v2+ makes this a hard error with mitigation tool (`namako stub`)

---

## Part 9: The AI-Assisted Spec-Driven Development Loop (v1)

### 9.1 Core Principle

**Namako is the authority.** The agent does not "guess correctness." It repeatedly:
```
run → classify → minimal edit → rerun
```
until all gates are satisfied.

### 9.2 Non-Negotiable Rules

1. **Never update certification implicitly.**
   - The agent MUST NOT run `namako update-cert` without explicit developer approval.

2. **Always lint before run.**
   - The agent MUST run `namako lint` and resolve all lint failures before `namako run`.

3. **One failure bucket at a time.**
   - The agent MUST classify failures and fix the smallest change that eliminates that bucket before continuing.

### 9.3 The Tight Loop (Slice-Based Workflow)

Work in **small slices** (typically one `Rule` or a small set of scenarios). Do not expand scope until the current slice is certified.

#### Step 1: Requirements Capture

**Goal:** Convert an idea into a testable behavioral contract.

**Exit condition:** Developer confirms the behavior description is correct and complete.

#### Step 2: Convert to Normative Spec (.feature)

**Goal:** The `.feature` file becomes the single normative spec surface.

**Agent actions:**
- Convert requirements into `.feature` file
- Put rationale into Gherkin comments (`# ...`)
- The `.feature` is now normative source

#### Step 3: Scenario Integrity Loop

**Goal:** Ensure `.feature` is structurally valid and unambiguous.

**Agent loop:**
1. Run: `namako lint`
2. If lint fails: fix and iterate

**Exit condition:** `namako lint` passes with no errors.

#### Step 4: Binding/Test Faithfulness Loop

**Goal:** Ensure scenarios are faithfully bound and executable.

**Agent loop:**
1. Run: `namako lint`
2. Run: `namako run`
3. Run: `namako verify`

**On failure:**
- Resolution/signature fails → fix bindings
- Execution faithfulness fails → fix adapter/bindings
- Verify fails → produce diff, explain drift, await developer approval before `update-cert`

**Exit condition:** All three pass.

#### Step 5: Implement the System

**Goal:** Implement/modify system under test until scenarios pass.

**Agent loop:**
- Make minimal implementation changes
- Re-run `namako lint` → `namako run` until green

**Exit condition:** All scenarios pass.

### 9.4 Existing Markdown Specs

This project has existing Markdown docs describing Naia behavior.
- Those docs are **source material only**
- `.feature` becomes **normative source**
- Markdown may be archived or deleted after conversion

---

## Part 10: Namako v2+ — Armor Plating (Deferred Publish-Grade Features)

This section captures all hardening features not required in v1 but designed into the system for future adoption.

### 10.1 FeatureAstNorm (Full AST Normal Form Hashing)

**What it adds:**
- Parse `.feature` → Gherkin AST → Canonical internal model (`FeatureAstNorm`) → Hash
- Immune to cosmetic changes (whitespace, comments, blank lines)

**Why it matters:**
- Publish-grade stability: spec identity survives formatting changes
- Enables meaningful diff on semantic changes only

**Migration:**
- Bump `hash_contract_version`
- Regenerate `certification.json`
- All identity fields will change

**FeatureAstNorm Schema:**
- Feature: `feature_id`, `feature_tags` (sorted), `rules[]` (ordered)
- Rule: `rule_id`, `rule_tags` (sorted), `background_steps[]`, `scenarios[]` (ordered)
- Scenario: `scenario_id`, `scenario_tags` (sorted), `steps[]`
- Scenario Outline: same as Scenario, plus `examples` as `BTreeMap<EID, ExampleRowNorm>`
- Step: `effective_kind`, `step_text`, `docstring`, `datatable`

### 10.2 Explicit Identity Tags

**What it adds:**
- `@FID(name)` on Features — explicit, refactor-stable feature identity
- `@Rnn` on Rules — explicit rule identity
- `@Snn` on Scenarios — explicit scenario identity
- `EID` column in Scenario Outline examples — explicit row identity

**Why it matters:**
- Identity survives file renames
- Identity survives scenario reordering
- Publish-grade stability for long-lived specs

**Migration:**
- Add required tags to all `.feature` files
- Bump `hash_contract_version`
- Regenerate certification

**Invariant (v2+):** Every Feature, Rule, Scenario, and Example Row MUST have explicit identity.

### 10.3 Orphan Bindings as Hard Error

**What it adds:**
- Binding in registry not used by any scenario → hard error

**Why it matters:**
- Prevents dead code in test bindings
- Ensures registry is minimal and intentional

**Mitigation tool:**
- `namako stub --binding <id>` generates a minimal placeholder scenario

**Migration:**
- Run `namako stub` for any orphans
- Or delete unused bindings

### 10.4 Challenge Packets / `namako review`

**What it adds:**
- `namako review` generates "challenge packets" for ambiguous expressions
- Forces developer to create discriminating scenarios

**Why it matters:**
- Proactive ambiguity detection
- Documentation-quality spec coverage

**Migration:**
- Run `namako review` periodically
- Address generated challenges

### 10.5 Canonical Byte Encoding (CBOR Profile)

**What it adds:**
- CBOR canonical encoding instead of JSON
- Strict schema enforcement
- Deterministic byte-for-byte output

**Why it matters:**
- True cross-implementation reproducibility
- Smaller artifact size
- More robust against encoding edge cases

**Migration:**
- Bump `hash_contract_version` to indicate CBOR
- Regenerate all artifacts
- Update all tooling to CBOR

### 10.6 Conformance Fixtures

**What it adds:**
- Fixture suite with:
  - Canonical input (structured)
  - Canonical encoded bytes
  - Expected hash outputs
- CI validates fixtures on all platforms

**Why it matters:**
- Proves cross-platform hash reproducibility
- Catches encoding bugs early

**Scope:**
- FeatureAstNorm
- SemanticStepRegistry
- ResolvedPlan

### 10.7 Resolution Semantics ID

**What it adds:**
- `resolution_semantics_id` field in identity tuple
- Stable string identifying: parsing + matching + kind inference + signature enforcement

**Why it matters:**
- Detects when resolution semantics change
- Enables controlled migration between resolution versions

**Initial value:** `"namako-resolution-v2"`

### 10.8 Rich `namako status` Diffs

**What it adds:**
- Detailed diff output showing:
  - Identity fields that changed (blocking)
  - Metadata fields that changed (informational)
  - Per-scenario/per-step breakdown

**Why it matters:**
- Developer UX for understanding drift
- Faster debugging

### 10.9 Stronger `impl_hash` Schemes

**What it adds:**
- Exclude comments from source fingerprint
- Exclude file paths from source fingerprint
- Capture dependency signals (imports, called functions)

**Why it matters:**
- `impl_hash` changes only when behavior changes
- Fewer false positives on cosmetic code changes

### 10.10 `bindings_used_hash`

**What it adds:**
- `bindings_used_hash` in identity tuple
- Computed from sorted list of unique binding IDs in resolved plan

**Why it matters:**
- Quick signal that binding set changed
- Enables fast-path verification

### 10.11 Multi-Language Support (Language-Neutral Engine, Language-Specific Adapters)

This section defines how Namako supports projects in **any programming language** (JS/TS, Python, Go, C++, JVM, .NET, etc.).

#### 10.11.1 Core Principle (Normative)

- The Namako Engine/CLI MUST remain a Rust tool.
- Any project integrates via an **external adapter executable** that implements NPAP.
- The adapter protocol is the **only cross-language integration boundary**.

**Engine Constraints:**
- The engine MUST NOT depend on project language runtimes.
- The engine MUST invoke adapters via `adapter_cmd` (configured in `namako.toml`).
- The engine MUST validate adapter outputs against strict JSON schemas.

**Adapter Constraints:**
- The adapter MUST implement `manifest` and `run` commands.
- The adapter MUST dispatch by `binding_id` only (no runtime text matching).
- The adapter MUST emit artifacts conforming to NPAP schemas.

#### 10.11.2 Universal "3-Piece" Project Pattern

Any language ecosystem SHOULD follow this pattern (equivalent to Naia's Rust structure):

| Component | Purpose | Naia Equivalent |
|-----------|---------|----------------|
| `<project>_test_harness` | World type + test helpers | `naia_test_harness` |
| `<project>_tests` | Step definitions (one keyword + one string per step) | `naia_tests` |
| `<project>_namako` | Adapter executable (`manifest` + `run`) | `naia_namako` |

**Language-Specific Examples:**

**JavaScript/TypeScript (Node.js):**
```
myproject-test-harness/   # npm package: World class, test utilities
myproject-tests/          # npm package: step definitions using decorators
myproject-namako/         # Node CLI: dist/myproject_namako.js
```

**Python:**
```
myproject_test_harness/   # Python package: World class, fixtures
myproject_tests/          # Python package: step definitions using decorators
myproject_namako/         # Python module: python -m myproject_namako
```

**Go:**
```
pkg/testharness/          # Go package: World struct, test helpers
pkg/tests/                # Go package: step definitions using struct tags or registration
cmd/myproject-namako/     # Go binary: ./bin/myproject-namako
```

**C++:**
```
src/test_harness/         # C++ library: World class, test utilities
src/tests/                # C++ library: step definitions via registration macros
src/myproject_namako/     # C++ binary: ./build/myproject_namako
```

#### 10.11.3 Adapter Command Configuration Examples

The `namako.toml` file configures the adapter command for each project:

```toml
# Rust (current Naia setup)
adapter_cmd = ["cargo", "run", "-q", "-p", "naia_namako", "--"]

# JavaScript/TypeScript (Node.js)
adapter_cmd = ["node", "dist/myproject_namako.js"]

# Python
adapter_cmd = ["python", "-m", "myproject_namako"]

# Go (compiled binary)
adapter_cmd = ["./bin/myproject-namako"]

# C++ (compiled binary)
adapter_cmd = ["./build/myproject_namako"]
```

> **Note:** These examples are v2+ guidance. v1 ships with Rust adapter support only.

### 10.12 Adapter SDKs (v2+)

**What it adds:**
- Official Namako SDKs for major ecosystems: JS/TS, Python, Go, JVM, .NET, C++

**Why it matters:**
- Without SDKs, each adapter author re-invents the protocol and risks subtle incompatibilities.
- SDKs ensure consistent UX and correct implementation across ecosystems.

**SDK Responsibilities (Normative):**

Each SDK MUST provide:

1. **Ergonomic Step Registration**
   - Functions/decorators/annotations consistent with: one keyword (Given/When/Then) + one string expression
   - Example (Python): `@given("a user named {string}")` 
   - Example (JS/TS): `Given("a user named {string}", async (world, name) => { ... })`

2. **Deterministic Binding ID Generation**
   - Compute `binding_id` from `(kind, expression_string)` using the documented `binding_id_scheme`
   - MUST produce identical IDs to the Rust implementation for the same inputs

3. **Semantic Registry Export**
   - Emit JSON manifest matching NPAP schema
   - Include `binding_id`, `kind`, `expression`, `signature`, `impl_hash`

4. **Plan Execution Harness**
   - Load `resolved_plan.json`
   - Dispatch steps by `binding_id` only (no text matching)
   - Invoke bindings with correct captures, docstrings, datatables

5. **Run Report Emission**
   - Emit `run_report.json` with canonical ordering
   - Include all required fields per NPAP schema

**Migration:**
- SDK adoption is optional but recommended
- Projects MAY implement NPAP directly without SDK

### 10.13 Cross-Language Hashing & Conformance (v2+)

Cross-language hash reproducibility is critical. This section defines two strategies.

#### Strategy 1: Reference Hash Helper ("Hash Oracle") — Recommended First

**What it adds:**
- A portable helper tool: `namako_hash_cli` (or `namako_hashd` daemon)
- Built from Rust, distributed as a standalone binary
- Adapters call it to compute hashes

**Contract (Normative):**
- The helper MUST implement the current `hash_contract_version` exactly.
- The helper MUST be distributed with version alignment to the Namako CLI.
- Adapters MUST declare in their manifest whether they use the helper (`hash_mode: "oracle"`) or native hashing (`hash_mode: "native"`).

**Helper Commands:**
```bash
# Compute binding_id
namako_hash_cli binding-id --kind Given --expression "a user named {string}"
# Output: {"binding_id": "abc123..."}

# Compute step_registry_hash
namako_hash_cli registry-hash --input registry.json
# Output: {"step_registry_hash": "def456..."}

# Compute payload_hash
namako_hash_cli payload-hash --input payload.json
# Output: {"payload_hash": "ghi789..."}
```

**Why it matters:**
- Ensures identical hashes across ecosystems without re-implementing canonical encoding.
- Reduces SDK implementation burden.
- Single source of truth for hash computation.

#### Strategy 2: Native Hashing in SDKs — Later (Publish Polish)

**What it adds:**
- SDKs implement canonical encoding + hashing natively in each language.
- No external helper dependency.

**Requirements:**
- SDKs MUST pass all conformance fixtures.
- SDKs MUST document their canonical encoding implementation.

#### Conformance Fixtures (Normative)

The Namako repo MUST ship conformance fixtures for:

| Fixture Category | Purpose |
|-----------------|--------|
| `binding_id_scheme` | Verify `(kind, expression)` → `binding_id` |
| `registry_hash` | Verify semantic registry → `step_registry_hash` |
| `payload_hash` | Verify execution payload → `payload_hash` |
| `plan_hash` | Verify resolved plan → `resolved_plan_hash` |

**Fixture Format (Normative):**
```json
{
  "fixture_version": "1",
  "hash_contract_version": "namako-v1-json+blake3-256",
  "cases": [
    {
      "name": "simple_given_step",
      "input": { "kind": "Given", "expression": "a user named {string}" },
      "expected_binding_id": "abc123..."
    }
  ]
}
```

**Validation:**
- Adapters/SDKs MUST be able to run the conformance suite.
- Any mismatch MUST cause the conformance check to fail.
- CI MUST validate fixtures on all supported platforms.

### 10.14 Adapter Certification Tooling (v2+)

**What it adds:**
- A CLI command: `namako adapter-verify` (or `namako conformance`)
- Validates third-party adapters before they are trusted in CI.

**Checks Performed (Normative):**

| Check | Description |
|-------|-------------|
| **Schema Validation** | Manifest and run_report match NPAP JSON schemas exactly |
| **Binding ID Correctness** | All `binding_id` values match expected computation from `(kind, expression)` |
| **Canonical Ordering** | Run report scenarios and steps are correctly ordered |
| **Hash Implementation** | All hashes match conformance fixture expectations |
| **Freshness Check** | Adapter correctly rejects stale plans |

**Output:**
- Clear pass/fail diagnostics per check.
- Detailed error messages for failures.

**Why it matters:**
- Ensures third-party adapters behave correctly.
- Catches protocol violations before they cause CI failures.
- Builds trust in the multi-language ecosystem.

**Usage:**
```bash
# Run adapter conformance suite
namako adapter-verify --adapter-cmd "node dist/myproject_namako.js"

# Run with specific fixtures
namako adapter-verify --adapter-cmd "./bin/myproject-namako" --fixtures path/to/fixtures/
```

---

## Part 11: Definition of Done (v1)

The v1 system is live when:

| Criterion | Description |
|-----------|-------------|
| **Resolution works** | `namako lint` resolves all features with strict errors |
| **Plan-driven execution works** | `namako run` executes via adapter by binding ID only |
| **Certification works** | `certification.json` contains identity tuple |
| **CI gate works** | `namako verify` passes in CI |
| **Manual update works** | `namako update-cert` refuses unless prerequisites met |
| **Adapter is non-autonomous** | Adapter dispatches by binding ID, no text matching |
| **Stale plans rejected** | Adapter refuses mismatched `step_registry_hash` |

---

## Appendix: No-drop Checklist (v9 Concept Trace)

This appendix traces every major concept from `NORTH_STAR_PLAN_v9.md` and labels its status.

### Goals

| Concept | Status | Notes |
|---------|--------|-------|
| Goal 1: Spec Unambiguity | **IN v1** (partial) | Operational ambiguity → hard error. `namako review` **DEFERRED** to v2+ (§10.4) |
| Goal 2: Scenario Completeness | **IN v1** (partial) | Structural completeness (resolve all steps). Deep coverage **DEFERRED** to v2+ |
| Goal 3: Test Faithfulness | **IN v1** | Plan-driven execution |
| Goal 4: Repeatable Perfection | **IN v1** | `namako verify` in CI |
| Goal 5: Change Propagation | **IN v1** | Hash-based identity |
| Goal 6: Audit-Grade Outputs | **IN v1** (partial) | Artifacts produced. Conformance fixtures **DEFERRED** to v2+ (§10.6) |

### Architecture

| Concept | Status | Notes |
|---------|--------|-------|
| Engine resolves, Adapter obeys | **IN v1** | Core principle |
| Trust boundary | **IN v1** | Trusted adapter assumption |
| Baseline vs Candidate | **IN v1** | Core certification model |
| Shared hashing infrastructure | **IN v1** | Canonical JSON for v1 |
| `namako_hash` crate | **DEFERRED** to v2+ | v1 uses inline hashing; v2+ may extract crate |

### Resolution & Plan

| Concept | Status | Notes |
|---------|--------|-------|
| Resolved Execution Plan | **IN v1** | Core artifact |
| `resolved_plan_hash` | **IN v1** | Core identity field |
| Kind inference (And/But → effective) | **IN v1** | Standard Gherkin semantics |
| Signature enforcement | **IN v1** | Hard error on mismatch |
| Strict ambiguity policy | **IN v1** | >1 match → hard error |
| Orphan → hard error | **DEFERRED** to v2+ | v1 may warn only (§10.3) |
| Missing step → hard error | **IN v1** | 0 matches → hard error |

### ID Scheme

| Concept | Status | Notes |
|---------|--------|-------|
| `@FID` feature identity | **DEFERRED** to v2+ | §10.2 |
| `@Rnn` rule identity | **DEFERRED** to v2+ | §10.2 |
| `@Snn` scenario identity | **DEFERRED** to v2+ | §10.2 |
| `EID` example row identity | **DEFERRED** to v2+ | §10.2 |
| Expression-based binding ID | **IN v1** | §4.2 |

### Spec Surface

| Concept | Status | Notes |
|---------|--------|-------|
| FeatureAstNorm | **DEFERRED** to v2+ | §10.1. v1 uses simpler fingerprint |
| `feature_ast_hash` | **DEFERRED** to v2+ | v1 uses `feature_fingerprint_hash` |
| Rule-only scenarios | **DEFERRED** to v2+ | v1 does not enforce |
| Background under Rule only | **DEFERRED** to v2+ | v1 does not enforce |
| Durations excluded from hash | **IN v1** | Durations are metadata only |

### Invariants (v9)

| Invariant | Status | Notes |
|-----------|--------|-------|
| 1: Structural Tag Integrity | **DEFERRED** to v2+ | Requires explicit ID scheme |
| 2: Explicit Binding Identity | **IN v1** | Generated binding ID |
| 3: Engine Supremacy | **IN v1** | Core principle |
| 4: No Orphan Bindings | **DEFERRED** to v2+ | v1 warns only |
| 5: Operational Determinism | **IN v1** | Sorted keys, stable order |
| 6: Single-Kind Binding Functions | **IN v1** | Each binding → one kind |
| 7: Collision-Free Execution | **IN v1** | Per-scenario World |
| 8: Explicit Certification Workflow | **IN v1** | `verify` checks, `update-cert` changes |

### NPAP

| Concept | Status | Notes |
|---------|--------|-------|
| `adapter manifest` | **IN v1** | Semantic registry |
| `adapter run --plan` | **IN v1** | Plan-driven execution |
| Semantic vs Debug registry split | **IN v1** (simplified) | v1 has semantic only; debug is optional |
| `impl_hash` | **IN v1** | Pragmatic approach |
| Freshness check | **IN v1** | Refuse stale plans |
| `executed_payload_hash` | **IN v1** | Integrity evidence |
| `executed_impl_hash` | **IN v1** | Drift signal |

### Certification

| Concept | Status | Notes |
|---------|--------|-------|
| Identity vs Metadata split | **IN v1** | Core design |
| `hash_contract_version` | **IN v1** | Versioned encoding |
| `resolution_semantics_id` | **DEFERRED** to v2+ | §10.7 |
| `bindings_used_hash` | **DEFERRED** to v2+ | §10.10 |
| Conformance fixtures | **DEFERRED** to v2+ | §10.6 |

### CLI

| Concept | Status | Notes |
|---------|--------|-------|
| `namako manifest` | **IN v1** | Debug command |
| `namako lint` | **IN v1** | Core command |
| `namako run` | **IN v1** | Core command |
| `namako verify` | **IN v1** | CI gate |
| `namako update-cert` | **IN v1** | Manual baseline update |
| `namako status` | **IN v1** (optional) | Diff tool |
| `namako review` | **DEFERRED** to v2+ | §10.4 |
| `namako stub` | **DEFERRED** to v2+ | §10.3 |

### Workflows

| Concept | Status | Notes |
|---------|--------|-------|
| Tight loop (AI-assisted SDD) | **IN v1** | §9 |
| Slice-based workflow | **IN v1** | §9.3 |
| Requirements capture | **IN v1** | Step 1 |
| Convert to .feature | **IN v1** | Step 2 |
| Scenario integrity loop | **IN v1** | Step 3 |
| Binding faithfulness loop | **IN v1** | Step 4 |
| Implement system | **IN v1** | Step 5 |

### Multi-Language Support (New)

| Concept | Status | Notes |
|---------|--------|-------|
| Language-neutral adapter protocol | **IN v1** (conceptual) | NPAP is language-neutral by design; v1 ships Rust adapter only |
| Any-language adapter support | **DEFERRED** to v2+ | §10.11 |
| Universal 3-piece project pattern | **DEFERRED** to v2+ | §10.11.2 |
| Adapter SDKs (JS/TS, Python, Go, etc.) | **DEFERRED** to v2+ | §10.12 |
| Cross-language hashing (hash oracle) | **DEFERRED** to v2+ | §10.13 Strategy 1 |
| Cross-language hashing (native SDK) | **DEFERRED** to v2+ | §10.13 Strategy 2 |
| Conformance fixtures for adapters | **DEFERRED** to v2+ | §10.13 |
| Adapter certification tooling | **DEFERRED** to v2+ | §10.14 |

### Dropped Concepts

| Concept | Status | Reason |
|---------|--------|--------|
| Malicious adapter defense | **DROPPED** | Out of scope; trusted adapter assumption (v2+ adds conformance as mitigation) |
| Deep semantic coverage measurement | **DROPPED** | Non-goal; review-driven process only |
| Assertion meaningfulness measurement | **DROPPED** | Non-goal; out of scope |

---

*End of NAMAKO_PLAN_FINAL.md*
