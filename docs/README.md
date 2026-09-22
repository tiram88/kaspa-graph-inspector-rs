# KGI v2 documentation

This directory contains the KGI v2 architecture, decision records,
implementation planning, project evidence, and historical material.

## Current authority during reorganization

The documentation reorganization is in progress. Until the dispatch and
cutover are complete, the authority and reading order in [`AGENTS.md`](../AGENTS.md)
remain unchanged:

1. `architecture/handoff-2026-09-20.md` is the consolidated normative
   contract.
2. Existing focused architecture documents apply where they agree with that
   handoff.
3. Later accepted decisions may amend the architecture explicitly.
4. Open questions, implementation plans, reviews, audits, issue records, and
   chat archives do not silently alter the contract.

The target structure is recorded in
[`architecture/README.md`](architecture/README.md). Paths identified there as
targets do not acquire authority merely by being listed. Authority changes
only in the explicit cutover step after the handoff has been completely
dispatched and checked.

## Document classes

| Area | Purpose | Current authority |
|---|---|---|
| `architecture/` | Current system contracts and the active handoff | Normative according to `AGENTS.md` |
| `decisions/` | Settled, open, deferred, rejected, and superseded decision registers plus any standalone ADRs | Accepted ADRs and rejected/superseded status constrain work; current behavior remains in the focused owner |
| `future-work.md` | Work explicitly outside KGI v2 | Non-normative |
| `implementation-sequence.md` | Planned implementation order | Non-normative |
| `implementation-status.md` | Current implementation state | Non-normative |
| `reviews/` | Review evidence and findings | Non-normative |
| `audits/` | Historical reconciliation evidence | Non-normative |
| `rk-issues/` | Tracked rusty-kaspa issue records | Non-normative and outside normal role scope |
| `chats/` | Untracked local chat provenance | Non-normative |

## Target organization

The agreed reorganization will:

- make focused files under `architecture/` the complete current behavioral
  contract, with one owner for each concern;
- split the decision register by decision state;
- move implementation planning and status under `implementation/`;
- move handoffs and audits under `history/`; and
- retain reviews, rusty-kaspa issues, chat archives, and future work as
  distinct document classes.

These changes are being made and reviewed incrementally. Historical handoffs
will move only after every current contract has an identified focused owner
and the dispatched text has been reconciled with later accepted decisions.
