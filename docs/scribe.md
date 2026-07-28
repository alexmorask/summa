# Scribe (AI layer)

_Status: stub — deep dive pending._

**Purpose:** the AI layer. Advisory only — it drafts things that the
deterministic FP core must still validate before anything is committed. The
Scribe never mutates state directly.

**Why "Scribe":** a scribe drafts and records on behalf of an authority but has
no power to decide. The name states the guardrail.

**Guardrail principle:** the Scribe proposes, the deterministic core decides.

**Candidate touchpoints:**
- Natural-language pricing authoring (drafts the Policy DSL → a pure validator
  checks invariants before commit).
- "Explain my bill" assistant (answers grounded in replayed events, not
  free-form generation).
- Reconciliation triage (advisory summary of likely cause/severity of drift; a
  human reviews — the Scribe never auto-resolves).
- Pricing what-if (paired with re-rating replay; describe a hypothetical change,
  the system deterministically shows impact before rollout).

**To decide in the deep dive:** which touchpoints ship first, how drafts are
represented and validated, model/prompt design.
