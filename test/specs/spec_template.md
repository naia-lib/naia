# Spec: <Domain Name>

**Status:** Draft | Active | Deprecated
**Version:** v1
**Applies to:** server | client | both | adapter

One-sentence overview of what this spec covers.

---

## Scope & Vocabulary

**In scope:**
- <what this spec covers>

**Out of scope:**
- <what this spec does not cover, with links to other specs if relevant>

**Vocabulary:**
- **<Term>**: <definition>

---

## Contracts

Write each contract as a distinct, testable requirement. Use the structure below for each contract.

### [contract-id-01] — Short Title

<Normative text describing the behavior. Use RFC 2119 keywords: MUST, MUST NOT, MAY, SHOULD, SHALL.>

**Observables:**
- Server observes: <what the server can see>
- Client observes: <what clients can see>

**Obligations:**
- **t1**: <first testable behavior claim>
- **t2**: <second testable behavior claim (if contract has multiple distinct behaviors)>

**Notes (non-normative):**
- <clarifications, edge cases, or rationale - optional>

---

### [contract-id-02] — Another Contract

<Normative text>

**Obligations:**
- **t1**: <testable behavior>

---

## Test Obligations

List all test functions that verify the contracts in this spec:

- `contract-id-01.t1`: <test file/function reference or TODO>
- `contract-id-01.t2`: <test file/function reference or TODO>
- `contract-id-02.t1`: <test file/function reference or TODO>

---

## Policy B Reminder

**Every contract MUST have at least one obligation labeled `t1`.**

This ensures:
- Uniform structure across all specs
- Clear mapping to labeled test assertions
- Scalable adequacy review

See `README.md` → "Policy B: Obligations Are Mandatory" for details.
