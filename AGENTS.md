# KGI v2 collaboration contract

This repository uses three distinct collaboration roles. The repository is the durable source of truth; chat transcripts are not.

## Mandatory reading order

Before changing or reviewing this repository, read in this order:

1. This `AGENTS.md`.
2. `docs/architecture/handoff-2026-09-20.md` (the current consolidated contract).
3. `docs/architecture/overview.md` and every focused architecture document relevant to the work.
4. Accepted ADRs in `docs/decisions/`.
5. `docs/open-questions.md` when the work touches an unresolved choice.
6. `docs/implementation-status.md` and relevant reports in `docs/reviews/` for non-normative project state.

`docs/architecture/handoff-2026-09-17.md` is the verbatim source handoff for the original bootstrap. Consult it for provenance, but its superseded wording is not the current contract. Focused documents were extracted from that bootstrap and cover their named components; the consolidated handoff contains broader cross-component and API contracts. Where they differ, the later accepted decisions recorded in `handoff-2026-09-20.md` control; flag any ambiguity rather than blending conflicting rules.

Normative precedence is:

```text
20 September consolidated handoff and later accepted ADRs
    > older focused architecture where superseded
    > implementation and tests
    > implementation-status and review notes
```

## Audit material

`docs/audits/` contains historical reconciliation evidence and working
reports. Audit files are non-normative: they do not participate in
architecture precedence and must not be used as implementation or review
contracts. Trace every accepted audit outcome to the current architecture,
an accepted ADR, or `docs/open-questions.md` as appropriate.

Version control tracks the complete `docs/audits/` directory for historical
provenance. Tracking an audit does not give it normative authority.

## Chat archives

`docs/chats/` contains local chat transcript archives used only as provenance
when an architecture reconciliation explicitly requires them. The entire
directory is ignored by Git and remains untracked. Chat archives are
non-normative and do not participate in architecture precedence.

## rusty-kaspa issue records

`docs/rk-issues/` contains local, one-file-per-issue records for rusty-kaspa
issues discovered during KGI v2 work. Version control tracks the complete
directory so the issue register is visible to the project, but no collaboration
role owns it. Architecture, Implementation, and Review roles must ignore its
contents unless the user explicitly asks them to work on a specific issue
record. These records are non-normative and do not participate in KGI
architecture, implementation, or review precedence.

## Temporary architecture reconciliation hold

The 20 September handoff is being verified against
`docs/chats/initial_chat.md` before production implementation begins. For
decisions made before that handoff, the last clearly accepted position in the
original exchange is presumed to reflect the intended design unless a later
accepted decision supersedes it. If that evidence conflicts with the handoff,
do not implement the disputed rule.
Architecture must verify the decision and record its resolution in a focused
architecture document or ADR, then reconcile the handoff. The transcript and
audit reports are evidence for this work, not standalone implementation
specifications. Production code, tests, and migrations remain gated until the
reconciled architecture is reviewed and committed as a stable baseline.

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
