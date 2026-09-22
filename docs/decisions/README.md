# Architecture decision register

This directory records KGI v2 decisions by current status. A status file is a
register, not a second copy of the architecture. Complete current behavior
and the rationale needed to interpret it belong in the focused document named
by the entry. The registers record each item's status, summarize what was
decided or remains unresolved, and link to its focused owner. Standalone ADRs
may preserve historical decision rationale, but they do not become competing
behavioral owners. Rejected and superseded entries may retain the reason a
former design must not return.

| File | Meaning |
|---|---|
| [settled.md](settled.md) | Accepted outcomes and their current architecture owners. |
| [open.md](open.md) | Requirements that still need an architecture decision. |
| [deferred.md](deferred.md) | Constrained implementation choices intentionally left to implementation work. |
| [rejected.md](rejected.md) | Proposals considered and never accepted. |
| [superseded.md](superseded.md) | Former designs replaced by a later accepted design. |

`docs/future-work.md` remains separate: it contains work outside KGI v2 rather
than unresolved v2 decisions.

When a decision changes status, move its entry in the same change that updates
the owning architecture document. Rejected and superseded entries prevent
accidental reintroduction; they do not compete with the current owner. Open and
deferred entries never weaken settled constraints.

During the documentation migration, the authority rules in `AGENTS.md` and the
20 September consolidated handoff continue to apply until the explicit
architecture cutover.
