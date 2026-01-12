# Spec: <Human-Readable Name>
<One sentence: what this spec guarantees.>

**Status:** Draft | Active | Deprecated  
**Version:** v1  
**Related specs:** <links>  
**Applies to:** <server | client | both | adapter> (pick one or more)

---

## 1) Scope & Vocabulary

**In scope:**  
- <what this spec covers, concretely>

**Out of scope:**  
- <explicit exclusions + link to the spec that owns them>

**Vocabulary (only terms used normatively below):**  
- **<Term>**: <definition>  
- **<Term>**: <definition>

---

## 2) Contract (Rules)

Write the contract as **numbered rules**. Each rule MUST be testable.

**R1. <Short title>**  
MUST/MUST NOT/MAY/SHOULD: <precise behavior>  
Clarifies: <only if needed to avoid misread>  

**R2. <Short title>**  
MUST/MUST NOT/MAY/SHOULD: <precise behavior>

(keep going)

---

## 3) Contract IDs (Obligations)

Contract IDs are the stable “handles” for enforcement + audits.

**Contract ID format:** `<spec-slug>-<nn>`  
Examples: `entities-identity-01`, `client-owned-publication-06`  
Rules: IDs MUST be stable, MUST NOT be reused, MUST NOT be renumbered.

For each obligation:

### <spec-slug>-<nn> — <short title>
**Guarantee:** <one sentence, no ambiguity>  
**Covered by tests:**  
- `test/tests/<file>.rs::<test_fn>` (or `TODO` if not implemented yet)  
**Notes:** <only if needed>

---

## 4) Interfaces & Observability

Everything here is about what callers/observers can rely on.

**Operations / Inputs:**  
- `<op or message>` → <what it means, at the contract level>

**Visible outcomes:**  
- What the server can observe: <facts>  
- What a client can observe: <facts>  

**Errors / illegal use:**  
- If <bad call / impossible case>: MUST <return error | ignore | disconnect | etc>, MUST NOT panic (unless explicitly stated)

**Events / statuses (if any):**  
- `<event/status>` emitted/visible when <condition> (exactly-once rules if relevant)

---

## 5) Invariants & Non-Goals

**Always true:**  
- <invariant that must hold across all operations>

**Non-goals:**  
- <what this spec intentionally does not guarantee>