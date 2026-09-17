# KGI v2 collaboration contract

This repository uses three distinct collaboration roles. The repository is the durable source of truth; chat transcripts are not.

## Mandatory reading order

Before changing or reviewing this repository, read in this order:

1. This `AGENTS.md`.
2. `docs/architecture/overview.md`.
3. Every focused document in `docs/architecture/` relevant to the work.
4. Accepted ADRs in `docs/decisions/`.
5. `docs/open-questions.md` when the work touches an unresolved choice.
6. `docs/implementation-status.md` and relevant reports in `docs/reviews/` for non-normative project state.

`docs/architecture/handoff-2026-09-17.md` is the verbatim source handoff for this bootstrap. Consult it when checking completeness, provenance, or a suspected conflict.

Normative precedence is:

```text
accepted architecture and ADRs
    > implementation and tests
    > implementation-status and review notes
```

If repository evidence conflicts with accepted architecture, flag the conflict. Do not resolve it implicitly. Record accepted architecture changes immediately in a focused architecture document or ADR.

## Architecture role

The Architecture role owns normative architecture documents and ADRs. It preserves the distinction between architectural contracts and implementation details, records accepted changes durably, and keeps `docs/open-questions.md` limited to genuinely unresolved or deliberately deferred matters.

The Architecture role does not implement production code unless explicitly asked.

## Implementation role

The Implementation role implements accepted architecture, tests, and migrations. It must read the applicable architecture before editing and must not invent or silently simplify architecture.

When implementation exposes an ambiguity or conflict, stop at the architectural boundary, document the evidence, and ask the Architecture role for a decision. Implementation choices that do not change accepted semantics may be recorded in code, tests, or `docs/implementation-status.md` as appropriate.

## Review role

The Review role evaluates stable commits or diffs against accepted architecture, ADRs, and relevant tests. It is read-only unless explicitly asked to fix findings.

Review findings belong in `docs/reviews/` when a durable report is requested. Findings and review notes are evidence, not normative architecture changes.

## Scope rules

- Statements marked settled in the architecture documents are accepted constraints.
- Rejected or superseded designs in `docs/decisions/README.md` must not be reintroduced implicitly.
- Open implementation choices must preserve all settled contracts.
- KGI v2.1 candidates in `docs/future-work.md` are outside the KGI v2 implementation unless explicitly promoted through an architecture decision.
- Production work must not begin until the architecture bootstrap has been reviewed and committed as a stable baseline.
