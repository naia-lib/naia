# Naia Specifications

This directory contains **normative specifications** that define the expected behavior of the Naia networking engine. Specs are the source of truth for what the system MUST, MUST NOT, MAY, SHALL, and SHOULD do.

## Downstream Chain

Specs establish the normative contract that flows downstream:

```
specs/*.md (normative)
    ↓
test/E2E_TEST_PLAN.md (test coverage plan)
    ↓
test/tests/*.rs (E2E test implementations)
    ↓
Implementation (production code)
```

**Specs are authoritative**: If a test or implementation contradicts a spec, the spec is correct and the test/implementation must be fixed.

## Spec Rules

All specifications in this directory MUST follow these rules:

### 1. Finite-State Models
- **MUST** use explicit state lists (all possible states enumerated)
- **MUST** define state transitions explicitly (inputs → preconditions → effects)
- **MUST NOT** use ambiguous or implicit state definitions

### 2. Explicit Transitions
- **MUST** specify:
  - Inputs (what triggers the transition)
  - Preconditions (what must be true before the transition)
  - Effects (what changes, including state transitions)
  - Postconditions (what must be true after the transition)

### 3. Observability
- **MUST** specify what each client can observe
- **MUST** specify what the server observes
- **MUST** distinguish between observable state and internal state

### 4. Illegal Behavior
- **MUST** define what happens when preconditions are violated
- **MUST** specify error conditions and error handling
- **MUST NOT** leave illegal cases undefined

### 5. Normative Keywords
- **MUST** use RFC 2119 keywords: **MUST**, **MUST NOT**, **MAY**, **SHALL**, **SHOULD**, **SHOULD NOT**
- **MUST NOT** use ambiguous language like "should probably" or "might"
- **MUST** use definitive statements only

### 6. Glossary
- **MUST** define all terms once in a glossary section
- **MUST** use terms consistently throughout the spec
- **MUST NOT** introduce new terms without definition

### 7. No Ambiguity
- **MUST** avoid ambiguous statements
- **MUST** be explicit about edge cases
- **MUST** specify ordering when it matters

## Style Guide

Reference `OWNERSHIP_DELEGATION_AUTH.md` as the canonical example of spec style and structure.

## Template

Use `_template.md` as a starting point for new specifications.

## Test Obligations

Each spec MUST include a "Test Obligations" section that lists the E2E test names that verify the spec. This creates traceability from spec → test.
