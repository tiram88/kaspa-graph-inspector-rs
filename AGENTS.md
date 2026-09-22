# KGI v2 collaboration contract

This repository uses three distinct collaboration roles. The repository is the durable source of truth; chat transcripts are not.

## Mandatory reading order

Before changing or reviewing this repository, read in this order:

1. This `AGENTS.md`.
2. `docs/architecture/README.md` for architecture ownership and navigation.
3. `docs/architecture/overview.md` and every focused architecture document
   relevant to the work.
4. `docs/decisions/README.md`, the settled, rejected, and superseded
   registers, and any accepted standalone ADRs in `docs/decisions/`.
5. `docs/decisions/open.md` or `docs/decisions/deferred.md` when the work
   touches an unresolved requirement or implementation choice.
6. `docs/implementation-status.md` and relevant reports in `docs/reviews/` for non-normative project state.

The handoffs under `docs/history/handoffs/` preserve the original bootstrap
and consolidation record. They are provenance only and do not participate in
normative precedence. Current behavior and rationale belong to the focused
architecture owner identified by `docs/architecture/README.md`.

Normative precedence is:

```text
focused architecture and later accepted ADRs
    > implementation and tests
    > implementation-status and review notes
```

## Audit material

`docs/audits/` contains historical reconciliation evidence and working
reports. Audit files are non-normative: they do not participate in
architecture precedence and must not be used as implementation or review
contracts. Trace every accepted audit outcome to the current architecture,
an accepted ADR, or the applicable decision-status register as appropriate.

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

## Architecture conflicts

If repository evidence conflicts with accepted architecture, flag the conflict. Do not resolve it implicitly. Record accepted architecture changes immediately in a focused architecture document or ADR.

## Architecture role

The Architecture role owns normative architecture documents and ADRs. It
preserves the distinction between architectural contracts and implementation
details, records accepted changes durably, and keeps the open and deferred
decision registers limited to their stated status.

The Architecture role does not implement production code unless explicitly asked.

## Implementation role

The Implementation role implements accepted architecture, tests, and migrations. It must read the applicable architecture before editing and must not invent or silently simplify architecture.

When implementation exposes an ambiguity or conflict, stop at the architectural boundary, document the evidence, and ask the Architecture role for a decision. Implementation choices that do not change accepted semantics may be recorded in code, tests, or `docs/implementation-status.md` as appropriate.

## Review role

The Review role evaluates stable commits or diffs against accepted architecture, ADRs, and relevant tests. It is read-only unless explicitly asked to fix findings.

Review findings belong in `docs/reviews/` when a durable report is requested. Findings and review notes are evidence, not normative architecture changes.

## Scope rules

- Statements marked settled in the architecture documents are accepted constraints.
- Rejected or superseded designs in `docs/decisions/rejected.md` and
  `docs/decisions/superseded.md` must not be reintroduced implicitly.
- Open implementation choices must preserve all settled contracts.
- KGI v2.1 candidates in `docs/future-work.md` are outside the KGI v2 implementation unless explicitly promoted through an architecture decision.
- Open architecture requirements must be resolved before dependent production
  work proceeds.
